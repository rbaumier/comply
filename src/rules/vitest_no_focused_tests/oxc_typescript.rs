//! vitest-no-focused-tests oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_ROOTS: &[&str] = &["test", "it", "describe", "suite"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".only"])
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
        if member.property.name.as_str() != "only" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if !TEST_ROOTS.contains(&obj.name.as_str()) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}.only(...)` skips the rest of the suite — remove it before committing.",
                obj.name.as_str()
            ),
            severity: Severity::Error,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_test_only() {
        let src = r#"test.only("focused", () => {});"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_describe_only() {
        let src = r#"describe.only("section", () => {});"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_test_without_only() {
        let src = r#"test("ok", () => {});"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_other_only_methods() {
        let src = r#"arr.only(x => x);"#;
        assert!(run(src).is_empty());
    }
}
