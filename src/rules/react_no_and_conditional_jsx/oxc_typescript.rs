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
        // Exempt boolean predicate calls (e.g. isError(), hasPermission()).
        if is_boolean_predicate_call(&logical.left) {
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

// mirrors jsx_ensure_booleans::BOOLEAN_PREFIXES
const BOOLEAN_PREFIXES: &[&str] = &[
    "is", "has", "should", "can", "will", "did", "show", "hide",
    "enable", "disable", "visible", "active", "open", "loading",
    "loaded", "allow", "need", "must",
];

fn is_boolean_predicate_call(expr: &Expression) -> bool {
    let expr = expr.without_parentheses();
    if let Expression::CallExpression(call) = expr {
        if let Expression::Identifier(id) = &call.callee {
            let lower = id.name.as_str().to_lowercase();
            return BOOLEAN_PREFIXES.iter().any(|p| lower.starts_with(p));
        }
    }
    false
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
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

    #[test]
    fn allows_type_guard_call() {
        assert!(run_on("const x = <div>{isErrorWithMessage(error) && <Panel />}</div>;").is_empty());
    }

    #[test]
    fn allows_boolean_returning_function() {
        assert!(run_on("const x = <div>{isTruthy(val) && <Panel />}</div>;").is_empty());
    }

    #[test]
    fn flags_numeric_expression() {
        assert_eq!(
            run_on("const x = <div>{count && <span>{count}</span>}</div>;").len(),
            1
        );
    }
}
