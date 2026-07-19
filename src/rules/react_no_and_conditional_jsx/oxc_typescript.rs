//! react-no-and-conditional-jsx oxc backend for TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::boolean_prefix::has_boolean_prefix;
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
            severity: Severity::Error,
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

// True when the operand can only evaluate to a boolean, so `expr && <X />`
// cannot leak a literal `0`/`''`. Covers boolean-prefixed predicates (calls
// like isError(), bare identifiers like withTimeBubble) plus syntactic forms
// that are always boolean: comparison/relational binary expressions, logical
// NOT (also covering `!!x`), and `typeof`. A `&&`/`||` chain is boolean when
// both operands are themselves boolean (`hasFilters && onClear !== undefined`).
// A member access is boolean when it is a Vue `.value` unwrap of a boolean base
// (`hasSorter.value`) or a boolean-prefixed property (`props.showHeader`).
fn is_boolean_predicate(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::CallExpression(call) => {
            matches!(&call.callee, Expression::Identifier(id) if has_boolean_prefix(id.name.as_str()))
        }
        Expression::Identifier(id) => has_boolean_prefix(id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            // A Vue `ref`/`computed` holding a boolean is read through `.value`,
            // so `hasSorter.value` is boolean when the unwrapped object is
            // (recurse to also cover nested bases like `cfg.isVirtual.value`).
            // A member whose own property follows the boolean-naming convention
            // (`props.showHeader`) is boolean by convention.
            if member.property.name.as_str() == "value" {
                is_boolean_predicate(&member.object)
            } else {
                has_boolean_prefix(member.property.name.as_str())
            }
        }
        Expression::BinaryExpression(binary) => {
            binary.operator.is_equality() || binary.operator.is_compare()
        }
        Expression::UnaryExpression(unary) => {
            unary.operator.is_not() || unary.operator.is_typeof()
        }
        Expression::LogicalExpression(logical) => {
            use oxc_ast::ast::LogicalOperator;
            matches!(logical.operator, LogicalOperator::And | LogicalOperator::Or)
                && is_boolean_predicate(&logical.left)
                && is_boolean_predicate(&logical.right)
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

    #[test]
    fn allows_boolean_prefixed_identifier_camel_case() {
        // Regression for #1282: boolean-prefixed identifiers are booleans by
        // convention and cannot leak `0`/`''`.
        assert!(run_on("const x = <div>{isSelected && <div />}</div>;").is_empty());
        assert!(run_on("const x = <div>{hasFilters && <div />}</div>;").is_empty());
        assert!(run_on("const x = <div>{shouldRender && <div />}</div>;").is_empty());
        assert!(run_on("const x = <div>{canEdit && <div />}</div>;").is_empty());
    }

    #[test]
    fn allows_boolean_prefixed_identifier_chain() {
        // The whole `&&` chain is boolean: a boolean-prefixed identifier and a
        // comparison.
        assert!(
            run_on("const x = <div>{hasFilters && onClear !== undefined && <button />}</div>;")
                .is_empty()
        );
    }

    #[test]
    fn flags_non_camel_case_boolean_lookalike() {
        // Starts with `is`/`can` but not at a camelCase boundary, so it is not a
        // boolean by convention and stays flagged.
        assert_eq!(run_on("const x = <div>{island && <div />}</div>;").len(), 1);
        assert_eq!(run_on("const x = <div>{cancel && <div />}</div>;").len(), 1);
    }

    #[test]
    fn allows_vue_boolean_ref_value_unwrap() {
        // Regression for #7310: a boolean-prefixed Vue ref read through `.value`
        // is boolean and cannot leak `0`/`''`.
        assert!(
            run_on("const x = () => <span>{hasSorter.value && <div>i</div>}</span>;").is_empty()
        );
        // Nested boolean-prefixed base: `virtualConfig.isVirtualScroll.value`.
        assert!(
            run_on(
                "const y = () => <span>{virtualConfig.isVirtualScroll.value && <div>i</div>}</span>;"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_boolean_prefixed_property() {
        // A member whose own property follows the boolean-naming convention is
        // boolean (`props.showHeader`).
        assert!(
            run_on("const z = () => <span>{props.showHeader && <div>i</div>}</span>;").is_empty()
        );
    }

    #[test]
    fn flags_non_boolean_ref_value_unwrap() {
        // A non-boolean-named ref read through `.value` can still leak `0`/`''`.
        assert_eq!(
            run_on("const a = () => <span>{count.value && <div>i</div>}</span>;").len(),
            1
        );
    }

    #[test]
    fn flags_non_boolean_prefixed_property() {
        // A member whose property is not boolean-prefixed stays flagged.
        assert_eq!(
            run_on("const b = () => <span>{props.title && <div>i</div>}</span>;").len(),
            1
        );
    }
}
