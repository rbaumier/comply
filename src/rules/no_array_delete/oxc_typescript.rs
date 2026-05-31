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
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
}
