//! OxcCheck backend — flag `Promise.reject()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Promise"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "reject" {
            return;
        }
        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "Promise" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`Promise.reject()` — prefer returning error values or throwing typed errors."
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
mod gated_tests {
    use super::*;
    use crate::rules::test_helpers::run_rule_gated;

    #[test]
    fn skips_promise_reject_mock_fixture_in_test_dir() {
        // #5757 firing site (soft-delete-row-actions.test.tsx): a `vi.fn`
        // fixture rejecting to drive the component's error branch under test.
        // The rejected promise is the stimulus, not production error handling —
        // the central `skip_in_test_dir` gate must suppress it.
        let src = r#"const onDeactivateConfirm = vi.fn(() => Promise.reject(new Error("server error")));"#;
        assert!(
            run_rule_gated(&Check, src, "src/app/components/data-table/soft-delete-row-actions.test.tsx")
                .is_empty()
        );
    }

    #[test]
    fn flags_promise_reject_in_production() {
        // Negative-space guard: the same call in a production module is the
        // rule's genuine target — keep flagging.
        let src = r#"export function load() { return Promise.reject(new Error("boom")); }"#;
        assert_eq!(run_rule_gated(&Check, src, "src/app/lib/load.ts").len(), 1);
    }
}
