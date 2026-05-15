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
        if !ctx.source.contains("prisma") {
            return;
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
}
