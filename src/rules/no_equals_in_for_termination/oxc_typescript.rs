use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ForStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ForStatement(for_stmt) = node.kind() else {
            return;
        };
        let Some(test) = &for_stmt.test else {
            return;
        };
        if !contains_equality(test) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, for_stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`for` loop uses equality (`==`/`===`) in termination — use `<`, `<=`, `>`, or `>=` instead.".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

/// Recursively check if an expression contains `==` or `===` (but not `!=` / `!==`).
fn contains_equality(expr: &Expression) -> bool {
    match expr {
        Expression::BinaryExpression(bin) => {
            matches!(
                bin.operator,
                BinaryOperator::Equality | BinaryOperator::StrictEquality
            ) || contains_equality(&bin.left)
                || contains_equality(&bin.right)
        }
        Expression::LogicalExpression(log) => {
            contains_equality(&log.left) || contains_equality(&log.right)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_triple_equals() {
        assert_eq!(run_on("for (let i = 0; i === 10; i++) {}").len(), 1);
    }

    #[test]
    fn flags_double_equals() {
        assert_eq!(run_on("for (let i = 0; i == 10; i++) {}").len(), 1);
    }

    #[test]
    fn allows_less_than() {
        assert!(run_on("for (let i = 0; i < 10; i++) {}").is_empty());
    }

    #[test]
    fn allows_not_equals() {
        assert!(run_on("for (let i = 0; i !== 10; i++) {}").is_empty());
    }
}
