use std::sync::Arc;

use oxc_ast::ast::{BinaryOperator, Expression, LogicalOperator, TSType, TSTypeAnnotation};
use oxc_span::GetSpan;

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

#[derive(PartialEq, Clone, Copy)]
enum Nullish {
    Null,
    Undefined,
}

fn span_text(source: &str, span: oxc_span::Span) -> &str {
    &source[span.start as usize..span.end as usize]
}

/// If `expr` is the `null` literal or the `undefined` identifier, report which.
fn nullish_of(expr: &Expression) -> Option<Nullish> {
    match expr {
        Expression::NullLiteral(_) => Some(Nullish::Null),
        Expression::Identifier(id) if id.name.as_str() == "undefined" => Some(Nullish::Undefined),
        _ => None,
    }
}

/// Match one side of the logical expression: a `BinaryExpression` with the
/// expected operator comparing an operand against `null` or `undefined`.
/// Returns the operand's source text and which nullish value it was compared to.
fn nullish_comparison<'a>(
    expr: &'a Expression<'a>,
    source: &'a str,
    expected_op: BinaryOperator,
) -> Option<(&'a str, Nullish)> {
    let Expression::BinaryExpression(bin) = expr else {
        return None;
    };
    if bin.operator != expected_op {
        return None;
    }
    if let Some(kind) = nullish_of(&bin.right) {
        return Some((span_text(source, bin.left.span()).trim(), kind));
    }
    if let Some(kind) = nullish_of(&bin.left) {
        return Some((span_text(source, bin.right.span()).trim(), kind));
    }
    None
}

fn returns_type_predicate(annotation: Option<&TSTypeAnnotation>) -> bool {
    annotation.is_some_and(|ann| matches!(ann.type_annotation, TSType::TSTypePredicate(_)))
}

/// `true` when the nearest enclosing function or arrow of `node_id` has a
/// type-predicate (`x is T`) return-type annotation.
fn enclosing_function_returns_predicate(
    node_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    for kind in nodes.ancestor_kinds(node_id).skip(1) {
        match kind {
            AstKind::Function(func) => return returns_type_predicate(func.return_type.as_deref()),
            AstKind::ArrowFunctionExpression(arrow) => {
                return returns_type_predicate(arrow.return_type.as_deref());
            }
            _ => {}
        }
    }
    false
}

/// `true` when `expr_id` is the returned expression (modulo parentheses) of a
/// function whose return type is a type predicate (`x is T`). That function is
/// the canonical `isDefined`-style guard this rule's remediation points to, and
/// its body can only be written as the explicit null/undefined pair — so the
/// rule must not fire there.
fn is_type_predicate_return(
    expr_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    let mut parent = nodes.parent_node(expr_id);
    while matches!(parent.kind(), AstKind::ParenthesizedExpression(_)) {
        parent = nodes.parent_node(parent.id());
    }
    match parent.kind() {
        AstKind::ReturnStatement(_) => enclosing_function_returns_predicate(parent.id(), nodes),
        // Concise arrow body: `(v): v is T => v !== null && v !== undefined`
        // is FunctionBody > ExpressionStatement > <expr>.
        AstKind::ExpressionStatement(_) => {
            let body = nodes.parent_node(parent.id());
            if !matches!(body.kind(), AstKind::FunctionBody(_)) {
                return false;
            }
            match nodes.parent_node(body.id()).kind() {
                AstKind::ArrowFunctionExpression(arrow) if arrow.expression => {
                    returns_type_predicate(arrow.return_type.as_deref())
                }
                _ => false,
            }
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LogicalExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["undefined"])
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

        // `&&` pairs with `!==`; `||` pairs with `===`. `??` never matches.
        let (expected_op, joiner, op_str, loose) = match logical.operator {
            LogicalOperator::And => (BinaryOperator::StrictInequality, "&&", "!==", "!="),
            LogicalOperator::Or => (BinaryOperator::StrictEquality, "||", "===", "=="),
            LogicalOperator::Coalesce => return,
        };

        let Some((left_op, left_kind)) = nullish_comparison(&logical.left, ctx.source, expected_op)
        else {
            return;
        };
        let Some((right_op, right_kind)) =
            nullish_comparison(&logical.right, ctx.source, expected_op)
        else {
            return;
        };

        // Need one `null` and one `undefined` comparison on the same operand.
        if left_kind == right_kind || left_op != right_op || left_op.is_empty() {
            return;
        }

        // The return expression of a `v is T` type-predicate function is the
        // canonical guard the remediation recommends; its body has no other
        // valid spelling, so it is exempt.
        if is_type_predicate_return(node.id(), semantic.nodes()) {
            return;
        }

        let guard = if expected_op == BinaryOperator::StrictInequality {
            format!("isDefined({left_op})")
        } else {
            format!("!isDefined({left_op})")
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, logical.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Redundant nullish check: `{left_op} {op_str} null {joiner} {left_op} {op_str} undefined` \
                 — replace with a single strict, type-narrowing guard like `{guard}` \
                 (a `value is NonNullable<T>` helper), not `{left_op} {loose} null`."
            ),
            severity: super::META.severity,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_and_neq_null_undefined() {
        let d = run_on("if (x !== null && x !== undefined) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-redundant-null-undefined-check");
    }

    #[test]
    fn flags_or_eq_null_undefined() {
        assert_eq!(run_on("if (x === null || x === undefined) {}").len(), 1);
    }

    #[test]
    fn flags_reversed_undefined_first() {
        assert_eq!(run_on("if (x !== undefined && x !== null) {}").len(), 1);
    }

    #[test]
    fn flags_member_operand() {
        assert_eq!(
            run_on("if (obj.prop !== null && obj.prop !== undefined) {}").len(),
            1
        );
    }

    #[test]
    fn flags_literal_on_left() {
        assert_eq!(run_on("if (null !== x && undefined !== x) {}").len(), 1);
    }

    #[test]
    fn allows_different_operands() {
        assert!(run_on("if (x !== null && y !== undefined) {}").is_empty());
    }

    #[test]
    fn allows_single_null_check() {
        assert!(run_on("if (x !== null) {}").is_empty());
    }

    #[test]
    fn allows_double_null() {
        // Same operand but both `null` — a different, out-of-scope redundancy.
        assert!(run_on("if (x !== null && x !== null) {}").is_empty());
    }

    #[test]
    fn allows_mismatched_operators() {
        // `&&` with a `===` side is not the redundant pattern.
        assert!(run_on("if (x !== null && x === undefined) {}").is_empty());
    }

    #[test]
    fn allows_and_with_strict_equality() {
        // `&&` pairs with `!==`; `x === null && x === undefined` is always false,
        // a contradiction that is out of this rule's scope.
        assert!(run_on("if (x === null && x === undefined) {}").is_empty());
    }

    #[test]
    fn allows_loose_equality() {
        // Loose `!=` already covers both; this rule scopes to strict equality.
        assert!(run_on("if (x != null && x != undefined) {}").is_empty());
    }

    #[test]
    fn allows_nullish_coalescing() {
        assert!(run_on("const y = x ?? undefined;").is_empty());
    }

    // Regression for #737 — composition.ts:40 from the amadeo deslop run.
    #[test]
    fn flags_response_value_check() {
        let d = run_on("const ok = responseValue !== null && responseValue !== undefined;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("responseValue"));
    }

    // Regression for #964 — the rule recommends an `isDefined` type-predicate
    // helper, so it must not flag the only possible body of that helper.
    #[test]
    fn allows_is_defined_predicate_body() {
        let d = run_on(
            "function isDefined<TValue>(value: TValue | null | undefined): value is TValue {\n\
             \x20\x20return value !== null && value !== undefined;\n\
             }",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_is_nullish_predicate_body() {
        let d = run_on(
            "function isNullish<TValue>(value: TValue | null | undefined): value is null | undefined {\n\
             \x20\x20return value === null || value === undefined;\n\
             }",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_concise_arrow_predicate_body() {
        let d = run_on(
            "const isDefined = <T>(value: T | null | undefined): value is T => \
             value !== null && value !== undefined;",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_parenthesized_predicate_return() {
        let d = run_on(
            "function isDefined<T>(value: T | null | undefined): value is T {\n\
             \x20\x20return (value !== null && value !== undefined);\n\
             }",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn flags_boolean_function_return() {
        // Only type-predicate returns are exempt; an ordinary `: boolean`
        // function can call `isDefined(value)` instead.
        let d = run_on(
            "function hasValue(value: string | null | undefined): boolean {\n\
             \x20\x20return value !== null && value !== undefined;\n\
             }",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn flags_non_return_position_inside_predicate() {
        // Exemption covers the return expression only — a condition inside a
        // predicate function can still delegate to the guard.
        let d = run_on(
            "function isDefined<T>(value: T | null | undefined): value is T {\n\
             \x20\x20if (value !== null && value !== undefined) { return true; }\n\
             \x20\x20return false;\n\
             }",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }
}
