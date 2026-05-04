//! react-no-and-conditional-jsx oxc backend for TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LogicalExpression(logical) = node.kind() else {
            return;
        };
        if logical.operator != oxc_ast::ast::LogicalOperator::And {
            return;
        }
        // Must be inside a JSXExpressionContainer.
        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::JSXExpressionContainer(_)) {
            return;
        }
        // Right side must be JSX.
        if !is_jsx_expr(&logical.right) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "react-no-and-conditional-jsx".into(),
            message: "`{expr && <X />}` renders `0` or `''` when expr \
                      is falsy-but-not-false. Use a ternary: \
                      `{expr ? <X /> : null}`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_jsx_expr(expr: &Expression) -> bool {
    matches!(
        expr.without_parentheses(),
        Expression::JSXElement(_) | Expression::JSXFragment(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_and_conditional_jsx() {
        assert_eq!(
            run_on("const x = <div>{isAdmin && <Panel />}</div>;").len(),
            1
        );
    }

    #[test]
    fn allows_ternary() {
        assert!(run_on("const x = <div>{isAdmin ? <Panel /> : null}</div>;").is_empty());
    }

    #[test]
    fn does_not_flag_non_jsx_right_operand() {
        assert!(run_on("const x = <div>{a && b}</div>;").is_empty());
    }
}
