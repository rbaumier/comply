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
        // Exempt boolean-prefixed predicates: calls (isError(), hasPermission())
        // and identifier references (withTimeBubble, showThumb) whose name
        // follows the boolean-naming convention, hence `boolean | undefined`.
        if is_boolean_predicate(&logical.left) {
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
    "is", "has", "should", "can", "will", "did", "show", "hide", "with",
    "enable", "disable", "visible", "active", "open", "loading",
    "loaded", "allow", "need", "must",
];

fn has_boolean_prefix(name: &str) -> bool {
    let lower = name.to_lowercase();
    BOOLEAN_PREFIXES.iter().any(|p| lower.starts_with(p))
}

// True when the operand can only evaluate to a boolean, so `expr && <X />`
// cannot leak a literal `0`/`''`. Covers boolean-prefixed predicates (calls
// like isError(), bare identifiers like withTimeBubble) plus syntactic forms
// that are always boolean: comparison/relational binary expressions, logical
// NOT (also covering `!!x`), and `typeof`.
fn is_boolean_predicate(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::CallExpression(call) => {
            matches!(&call.callee, Expression::Identifier(id) if has_boolean_prefix(id.name.as_str()))
        }
        Expression::Identifier(id) => has_boolean_prefix(id.name.as_str()),
        Expression::BinaryExpression(binary) => {
            binary.operator.is_equality() || binary.operator.is_compare()
        }
        Expression::UnaryExpression(unary) => {
            unary.operator.is_not() || unary.operator.is_typeof()
        }
        _ => false,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_and_conditional_jsx() {
        assert_eq!(
            run_on("const x = <div>{admin && <Panel />}</div>;").len(),
            1
        );
    }

    #[test]
    fn allows_ternary() {
        assert!(run_on("const x = <div>{admin ? <Panel /> : null}</div>;").is_empty());
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

    #[test]
    fn allows_boolean_prefixed_identifier() {
        assert!(run_on("const x = <div>{withTimeBubble && <div />}</div>;").is_empty());
        assert!(run_on("const x = <div>{showThumb && <div />}</div>;").is_empty());
    }

    #[test]
    fn flags_non_boolean_identifier() {
        assert_eq!(run_on("const x = <div>{count && <div />}</div>;").len(), 1);
    }

    #[test]
    fn flags_length_member_expression() {
        assert_eq!(
            run_on("const x = <div>{items.length && <div />}</div>;").len(),
            1
        );
    }

    #[test]
    fn allows_comparison_expression() {
        assert!(run_on("const x = <div>{dev === null && <Dots />}</div>;").is_empty());
        assert!(run_on("const x = <div>{a !== b && <Box />}</div>;").is_empty());
    }

    #[test]
    fn allows_relational_expression() {
        assert!(run_on("const x = <div>{width > 0 && <View />}</div>;").is_empty());
        assert!(run_on("const x = <div>{n <= 10 && <View />}</div>;").is_empty());
    }

    #[test]
    fn allows_logical_not() {
        assert!(run_on("const x = <div>{!active && <Box2 />}</div>;").is_empty());
    }

    #[test]
    fn allows_double_negation() {
        assert!(run_on("const x = <div>{!!value && <Box />}</div>;").is_empty());
    }

    #[test]
    fn allows_typeof_expression() {
        assert!(run_on("const x = <div>{typeof x === 'string' && <Box />}</div>;").is_empty());
        assert!(run_on("const x = <div>{typeof x && <Box />}</div>;").is_empty());
    }
}
