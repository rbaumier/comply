//! no-array-delete oxc backend — flag `delete arr[i]`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UnaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["delete"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UnaryExpression(unary) = node.kind() else {
            return;
        };
        if unary.operator != oxc_ast::ast::UnaryOperator::Delete {
            return;
        }
        // Test files delete `process.env` keys and fixture entries in teardown —
        // bounded to the test scope with no non-mutating equivalent.
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        // The argument must be a computed member expression (bracket access).
        let Expression::ComputedMemberExpression(member) = &unary.argument else {
            return;
        };
        // Skip `delete process.env[key]` — process.env is NodeJS.ProcessEnv
        // (a dictionary), not an array, so this is property deletion, not
        // sparse-array creation.
        if is_process_env(&member.object) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`delete arr[i]` creates a sparse hole — use `arr.splice(i, 1)` instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True for the `process.env` member expression.
fn is_process_env(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(m) = expr else {
        return false;
    };
    m.property.name == "env"
        && matches!(&m.object, Expression::Identifier(id) if id.name == "process")
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
mod oxc_tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    #[test]
    fn flags_delete_array_element() {
        assert_eq!(run("delete arr[0];").len(), 1);
    }

    #[test]
    fn skips_delete_process_env_issue_479() {
        let src = "delete process.env[key];";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_in_test_file_issue_582() {
        // Test teardown deletes fixture entries; bounded to test scope.
        assert!(run_in_test_file("delete fixtures[id];").is_empty());
    }
}
