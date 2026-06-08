//! prisma-soft-delete-filter oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const FIND_METHODS: &[&str] = &["findMany", "findFirst", "findUnique"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".findMany(", ".findFirst(", ".findUnique("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !FIND_METHODS.contains(&method) {
            return;
        }
        // Only fire when the file mentions `prisma` somewhere — keeps
        // the rule from misfiring on Drizzle / unrelated APIs that may
        // happen to expose the same method name.
        if !ctx.source_contains("prisma") {
            return;
        }
        // When a schema.prisma is available, skip models that don't have a
        // `deletedAt` field — they can't have soft-deleted rows, so the
        // missing filter is not a bug. Fall through when no schema is found
        // (backward-compatible: fire on all).
        if let Expression::StaticMemberExpression(inner) = &member.object {
            let model_name = inner.property.name.as_str();
            if let Some(models) = ctx.project.prisma_soft_delete_models() {
                if !models.contains(model_name.to_lowercase().as_str()) {
                    return;
                }
            }
        }
        // Heuristic: scan the entire call source range for
        // `deletedAt` — present anywhere in the where clause is fine.
        let span_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if span_text.contains("deletedAt") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{method}` without a `deletedAt` filter — soft-deleted rows will \
                 leak into the result. Add `where: {{ deletedAt: null, … }}`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    fn run_with_project(src: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_project(src, &Check, project)
    }

    #[test]
    fn flags_find_many_without_deleted_at() {
        let src = r#"const r = await prisma.user.findMany({ where: { active: true } });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_find_many_with_deleted_at() {
        let src = r#"const r = await prisma.user.findMany({ where: { deletedAt: null, active: true } });"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_prisma_callers() {
        let src = r#"const r = obj.findMany({ where: { active: true } });"#;
        assert!(run(src).is_empty());
    }

    // Regression tests for issue #836: FP on models without deletedAt field.

    #[test]
    fn ignores_find_first_on_model_without_deleted_at_in_schema() {
        // schema contains "envelope" with deletedAt, but not "user"
        let project = ProjectCtx::for_test_with_prisma_models(&["envelope"]);
        let src =
            r#"const u = await prisma.user.findFirst({ where: { email: "x" } });"#;
        assert!(run_with_project(src, &project).is_empty());
    }

    #[test]
    fn flags_find_first_on_model_with_deleted_at_in_schema() {
        let project = ProjectCtx::for_test_with_prisma_models(&["envelope"]);
        let src =
            r#"const e = await prisma.envelope.findFirst({ where: { id: "1" } });"#;
        assert_eq!(run_with_project(src, &project).len(), 1);
    }
}
