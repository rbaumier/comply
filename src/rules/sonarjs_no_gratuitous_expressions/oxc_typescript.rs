//! sonarjs-no-gratuitous-expressions oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True if `expr` is a literal boolean (`true` / `false`) or a literal
/// that's always truthy / falsy at compile time (`0`, `""`, `null`,
/// `undefined`, non-empty string / non-zero number).
fn is_constant_condition(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::IfStatement,
            AstType::ConditionalExpression,
            AstType::WhileStatement,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (test_expr, span_start) = match node.kind() {
            AstKind::IfStatement(stmt) => (&stmt.test, stmt.span.start),
            AstKind::ConditionalExpression(c) => (&c.test, c.span.start),
            AstKind::WhileStatement(w) => (&w.test, w.span.start),
            _ => return,
        };
        if !is_constant_condition(test_expr) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Condition is a compile-time constant — one branch is dead. \
                      Remove it, or fix the condition to depend on runtime state."
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_if_true() {
        let src = "function f() { if (true) { return 1; } return 0; }";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn flags_while_false() {
        let src = "function f() { while (false) { break; } }";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_runtime_condition() {
        let src = "function f(x: boolean) { if (x) return 1; return 0; }";
        assert!(run(src).is_empty());
    }
}
