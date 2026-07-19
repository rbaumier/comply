//! js-no-math-spread-array OXC backend — flag `Math.min(...arr)` / `Math.max(...arr)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{
    byte_offset_to_line_col, expression_is_statically_bounded_array, span_contains,
};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BinaryOperator, Expression, LogicalOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Math"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "Math" {
            return;
        }
        let method = member.property.name.as_str();
        if method != "min" && method != "max" {
            return;
        }
        let spreads: Vec<&Expression> = call
            .arguments
            .iter()
            .filter_map(|a| match a {
                Argument::SpreadElement(s) => Some(&s.argument),
                _ => None,
            })
            .collect();
        if spreads.is_empty() {
            return;
        }
        // Spreading a statically-bounded array (literal, length-non-increasing
        // `.map`/`.filter`/`.slice` chain rooted at one, or a fixed-length tuple
        // binding) cannot exhaust the argument-count limit, so there is no
        // stack-overflow risk. The same guarantee holds for an identifier spread
        // whose enclosing branch is gated by an upper-bound `.length` check on
        // that identifier (`if (x.length === 2) … Math.max(...x)`): the guard caps
        // the element count statically. Only flag when some spread operand is
        // dynamic and unguarded.
        if spreads.iter().all(|operand| {
            expression_is_statically_bounded_array(operand, semantic)
                || spread_operand_is_length_guarded(operand, node, semantic)
        }) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`Math.{method}(...array)` overflows the stack on large arrays — \
                 use `reduce` or a for-loop instead."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Returns true when `operand` is an identifier binding whose enclosing guard
/// branch statically caps its element count via an upper-bound `.length` check
/// (so the spread expands to a known-small arity, not an unbounded array).
fn spread_operand_is_length_guarded(
    operand: &Expression,
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Some(target) = identifier_symbol(operand, semantic) else {
        return false;
    };
    nearest_guard_caps_length(node, semantic, target)
}

/// Walks the call's ancestor chain for a dominating guard whose condition caps
/// the `target` binding's length. A guard dominates the spread when the call sits
/// in:
///   - an `IfStatement` consequent (gated by the `if` test),
///   - a `ConditionalExpression` consequent (gated by the ternary test), or
///   - the right operand of a logical `&&` (gated by the left operand).
/// A length check in an unrelated sibling branch or a `||` alternative does not
/// dominate the call and is ignored.
fn nearest_guard_caps_length(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    target: oxc_semantic::SymbolId,
) -> bool {
    let call_span = node.kind().span();
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::IfStatement(if_stmt) => {
                if span_contains(if_stmt.consequent.span(), call_span)
                    && condition_caps_length(&if_stmt.test, semantic, target)
                {
                    return true;
                }
            }
            AstKind::ConditionalExpression(cond) => {
                if span_contains(cond.consequent.span(), call_span)
                    && condition_caps_length(&cond.test, semantic, target)
                {
                    return true;
                }
            }
            AstKind::LogicalExpression(logical) => {
                if logical.operator == LogicalOperator::And
                    && span_contains(logical.right.span(), call_span)
                    && condition_caps_length(&logical.left, semantic, target)
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Returns true when `expr`, used as a guarding condition, statically caps the
/// element count of the `target` binding through an upper-bound `.length`
/// comparison. Recurses through `&&` conjuncts and parentheses — every `&&`
/// operand must hold for the guarded branch to run, so any conjunct that caps the
/// length suffices. `||` is not a guarantee and is not traversed.
fn condition_caps_length(
    expr: &Expression,
    semantic: &oxc_semantic::Semantic,
    target: oxc_semantic::SymbolId,
) -> bool {
    match expr {
        Expression::LogicalExpression(logical) if logical.operator == LogicalOperator::And => {
            condition_caps_length(&logical.left, semantic, target)
                || condition_caps_length(&logical.right, semantic, target)
        }
        Expression::ParenthesizedExpression(paren) => {
            condition_caps_length(&paren.expression, semantic, target)
        }
        Expression::BinaryExpression(bin) => {
            // `target.length <op> N`: `===`/`==` pin an exact arity, `<`/`<=` cap
            // the maximum. A lower-bound-only guard (`length > 0`, `length >= 1`)
            // does not cap the maximum and must NOT exempt the spread.
            if is_length_of_symbol(&bin.left, semantic, target)
                && is_nonneg_int_literal(&bin.right)
            {
                return matches!(
                    bin.operator,
                    BinaryOperator::StrictEquality
                        | BinaryOperator::Equality
                        | BinaryOperator::LessThan
                        | BinaryOperator::LessEqualThan
                );
            }
            // Mirror `N <op> target.length`: `N >= len` / `N > len` cap the maximum.
            if is_length_of_symbol(&bin.right, semantic, target)
                && is_nonneg_int_literal(&bin.left)
            {
                return matches!(
                    bin.operator,
                    BinaryOperator::StrictEquality
                        | BinaryOperator::Equality
                        | BinaryOperator::GreaterThan
                        | BinaryOperator::GreaterEqualThan
                );
            }
            false
        }
        _ => false,
    }
}

/// Returns true when `expr` is `<binding>.length` where `<binding>` resolves to
/// the `target` symbol — the same array being spread, not a same-named binding in
/// another scope.
fn is_length_of_symbol(
    expr: &Expression,
    semantic: &oxc_semantic::Semantic,
    target: oxc_semantic::SymbolId,
) -> bool {
    matches!(expr, Expression::StaticMemberExpression(member)
        if member.property.name.as_str() == "length"
            && identifier_symbol(&member.object, semantic) == Some(target))
}

/// Resolves an identifier expression to the symbol it references, or `None` for a
/// non-identifier or an unresolved reference.
fn identifier_symbol(
    expr: &Expression,
    semantic: &oxc_semantic::Semantic,
) -> Option<oxc_semantic::SymbolId> {
    let Expression::Identifier(ident) = expr else {
        return None;
    };
    let ref_id = ident.reference_id.get()?;
    semantic.scoping().get_reference(ref_id).symbol_id()
}

/// Returns true when `expr` is a non-negative integer numeric literal — a static
/// element-count bound.
fn is_nonneg_int_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::NumericLiteral(num)
        if num.value.is_finite() && num.value >= 0.0 && num.value.fract() == 0.0)
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

    // Dynamic / unbounded spreads — genuine stack-overflow risk, still flagged.
    #[test]
    fn flags_spread_of_dynamic_array_param() {
        assert_eq!(run_on("function f(nums: number[]) { return Math.max(...nums); }").len(), 1);
    }

    #[test]
    fn flags_spread_of_function_return() {
        assert_eq!(run_on("Math.max(...getList());").len(), 1);
    }

    #[test]
    fn flags_spread_of_unannotated_param() {
        assert_eq!(run_on("function f(xs) { return Math.min(...xs); }").len(), 1);
    }

    #[test]
    fn flags_map_rooted_at_dynamic_array() {
        assert_eq!(
            run_on("function f(nums: number[]) { return Math.max(...nums.map(x => x)); }").len(),
            1
        );
    }

    // Statically-bounded spreads — no stack risk, not flagged.
    #[test]
    fn allows_spread_of_array_literal() {
        assert!(run_on("Math.max(...[a, b, c]);").is_empty());
    }

    #[test]
    fn allows_spread_of_map_rooted_at_literal() {
        assert!(run_on("Math.min(...[a, b, c, d].map(c => c.x));").is_empty());
    }

    #[test]
    fn allows_spread_of_bounded_literal_binding() {
        let src = "const corners = [a, b, c, d]; Math.min(...corners.map(c => c.x));";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_spread_of_tuple_typed_binding() {
        let src = "const p: [number, number] = getP(); Math.max(...p);";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Issue #5292: visgl/deck.gl — bbox corners spread to find an AABB.
    #[test]
    fn allows_deckgl_bbox_corners() {
        let src = r#"
            const transformedCoords = [
              modelMatrix.transformAsPoint([bbox[0], bbox[1]]),
              modelMatrix.transformAsPoint([bbox[2], bbox[1]]),
              modelMatrix.transformAsPoint([bbox[0], bbox[3]]),
              modelMatrix.transformAsPoint([bbox[2], bbox[3]]),
            ];
            const x = Math.min(...transformedCoords.map(i => i[0]));
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Issue #5292: visgl/deck.gl shadow.ts — frustum corners through chained maps.
    #[test]
    fn allows_deckgl_frustum_corners() {
        let src = r#"
            const corners = [[0,0,1],[1,0,1],[0,1,1],[1,1,1]].map(p => f(p));
            const positions = corners.map(c => g(c));
            const left = Math.min(...positions.map(p => p[0]));
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Negative space: `.flatMap` can grow the result beyond the bounded root, so
    // the spread is no longer provably bounded and must still flag.
    #[test]
    fn flags_flatmap_rooted_at_literal() {
        assert_eq!(run_on("Math.max(...[a, b].flatMap(x => x));").len(), 1);
    }

    // Negative space: a rest-element tuple `[number, ...number[]]` is unbounded,
    // so a binding typed as one must still flag.
    #[test]
    fn flags_rest_tuple_typed_binding() {
        let src = "const xs: [number, ...number[]] = getXs(); Math.max(...xs);";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Negative space: a `let` binding can be reassigned to a dynamic array after
    // its bounded literal initializer, so the literal arity is not load-bearing.
    #[test]
    fn flags_reassigned_let_binding() {
        let src = "let arr = [a, b]; arr = getHuge(); Math.max(...arr);";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Negative space: a `const` array literal grown in place via `.push` is an
    // accumulator with unknown final size, so it must still flag.
    #[test]
    fn flags_push_accumulator() {
        let src = "const arr = []; for (const x of xs) arr.push(x); Math.max(...arr);";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Issue #6496: sindresorhus/is — both spreads are gated by `range.length === 2`
    // in the enclosing `if`, a static upper bound on the element count.
    #[test]
    fn allows_spread_guarded_by_length_equality() {
        let src = r#"
            function isInRange(value, range) {
                if (isArray(range) && range.length === 2) {
                    return value >= Math.min(...range) && value <= Math.max(...range);
                }
            }
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // An upper-bound `<=` guard caps the arity just as `===` does.
    #[test]
    fn allows_spread_guarded_by_length_le() {
        let src = "function f(xs) { if (xs.length <= 8) return Math.max(...xs); }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // The mirrored upper bound `N > x.length` also caps the arity.
    #[test]
    fn allows_spread_guarded_by_mirrored_upper_bound() {
        let src = "function f(xs) { if (3 > xs.length) return Math.min(...xs); }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // An inline `&&` short-circuit guard caps the right-operand call.
    #[test]
    fn allows_spread_guarded_by_logical_and() {
        let src = "function f(xs) { return xs.length === 2 && Math.max(...xs); }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Negative space: a lower-bound-only guard (`length > 0`) does not cap the
    // maximum element count, so the spread must still flag.
    #[test]
    fn flags_spread_guarded_by_lower_bound_only() {
        let src = "function f(xs) { if (xs.length > 0) return Math.max(...xs); }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Negative space: a `>= 1` guard is also a lower bound only and stays flagged.
    #[test]
    fn flags_spread_guarded_by_length_ge_one() {
        let src = "function f(xs) { if (xs.length >= 1) return Math.max(...xs); }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Negative space: the length guard is on a DIFFERENT variable than the one
    // spread, so it does not bound the spread operand.
    #[test]
    fn flags_when_length_guard_targets_other_variable() {
        let src = "function f(xs, ys) { if (ys.length === 2) return Math.max(...xs); }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Negative space: a length check in a SIBLING branch does not dominate the
    // call in the else-path, so the unguarded spread still flags.
    #[test]
    fn flags_when_length_guard_in_sibling_branch() {
        let src = "function f(xs) { if (xs.length === 2) { g(); } else { return Math.max(...xs); } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Negative space: the guarded `.length` and the spread reference DIFFERENT
    // bindings that merely share a name — an inner `const xs` shadows the guarded
    // outer param — so the guard does not bound the spread operand. Name-equality
    // would wrongly suppress this; symbol identity keeps it flagged.
    #[test]
    fn flags_when_inner_shadow_breaks_symbol_identity() {
        let src = "function f(xs) { if (xs.length === 2) { const xs = getHuge(); return Math.max(...xs); } }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }
}
