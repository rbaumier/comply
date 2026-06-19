//! zod-no-string-schema-with-uuid oxc backend — flag `z.string().uuid()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.string"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression with property `uuid`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "uuid" {
            return;
        }

        // Object must be a call expression (the `z.string()` part).
        let Expression::CallExpression(inner_call) = &member.object else {
            return;
        };

        // Inner callee must be `z.string`.
        let Expression::StaticMemberExpression(inner_member) = &inner_call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &inner_member.object else {
            return;
        };
        if obj.name.as_str() != "z" || inner_member.property.name.as_str() != "string" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `z.uuid()` instead of `z.string().uuid()` — the \
                      chained form is deprecated in Zod v4."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::{run_rule, run_rule_gated};

    #[test]
    fn flags_string_uuid_chain() {
        let d = run_rule(&Check, "const u = z.string().uuid();", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("z.uuid()"));
    }

    #[test]
    fn allows_top_level_uuid() {
        assert!(run_rule(&Check, "const u = z.uuid();", "t.ts").is_empty());
    }

    // Issue #4450: zod's own v3 tests (where `z.string().uuid()` is the correct
    // v3 API) and v4/classic backward-compat tests (which deliberately exercise
    // the deprecated chained form) must not be nudged. The central
    // `skip_in_test_dir` gate exempts `/tests/` dirs and `.test.`/`.spec.` infixes.
    #[test]
    fn silent_in_test_dir_issue4450() {
        assert!(
            run_rule_gated(
                &Check,
                r#"const u = z.string().uuid("custom error");"#,
                "packages/zod/src/v3/tests/string.test.ts",
            )
            .is_empty()
        );
        assert!(
            run_rule_gated(&Check, "const u = z.string().uuid();", "src/schema.test.ts").is_empty()
        );
    }

    // Suppression is test-scoped: the same chained form in production source is
    // still flagged, preserving the rule's purpose.
    #[test]
    fn flags_string_uuid_chain_in_production_issue4450() {
        assert_eq!(
            run_rule_gated(&Check, "const u = z.string().uuid();", "src/schema.ts").len(),
            1
        );
    }
}
