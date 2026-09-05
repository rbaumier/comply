use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::{byte_offset_to_line_col, expression_is_string_array};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{CallExpression, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Matchers that compare their two operands structurally. Sorting both operands
/// canonicalises them for that comparison; `toBe` (reference identity) and the
/// snapshot matchers make no such use of an ordering.
const DEEP_EQUALITY_MATCHERS: &[&str] = &["toEqual", "toStrictEqual", "toMatchObject"];

/// Whether `call` is a `.sort()` invoked with no comparator.
fn is_comparator_less_sort(call: &CallExpression) -> bool {
    call.arguments.is_empty()
        && matches!(
            &call.callee,
            Expression::StaticMemberExpression(member) if member.property.name.as_str() == "sort"
        )
}

/// Whether computing `expr` runs a comparator-less `.sort()`: the expression
/// itself, or one anywhere in the receiver chain or arguments it is built from
/// (`a.map(f).sort()`, `a.sort().join('')`).
fn computes_comparator_less_sort(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            is_comparator_less_sort(call)
                || match &call.callee {
                    Expression::StaticMemberExpression(member) => {
                        computes_comparator_less_sort(&member.object)
                    }
                    Expression::ComputedMemberExpression(member) => {
                        computes_comparator_less_sort(&member.object)
                    }
                    _ => false,
                }
                || call
                    .arguments
                    .iter()
                    .filter_map(|arg| arg.as_expression())
                    .any(computes_comparator_less_sort)
        }
        Expression::StaticMemberExpression(member) => {
            computes_comparator_less_sort(&member.object)
        }
        Expression::ComputedMemberExpression(member) => {
            computes_comparator_less_sort(&member.object)
        }
        Expression::ParenthesizedExpression(paren) => {
            computes_comparator_less_sort(&paren.expression)
        }
        Expression::AwaitExpression(await_expr) => {
            computes_comparator_less_sort(&await_expr.argument)
        }
        Expression::TSAsExpression(as_expr) => computes_comparator_less_sort(&as_expr.expression),
        Expression::TSSatisfiesExpression(sat) => computes_comparator_less_sort(&sat.expression),
        Expression::TSNonNullExpression(nn) => computes_comparator_less_sort(&nn.expression),
        _ => false,
    }
}

/// The value handed to `expect(…)` at the root of a matcher chain, whatever
/// modifiers (`.not`, `.resolves`, `.rejects`) sit between it and the matcher.
fn expect_operand<'a>(callee_object: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    match callee_object {
        Expression::CallExpression(call)
            if matches!(&call.callee, Expression::Identifier(id) if id.name.as_str() == "expect") =>
        {
            call.arguments.first().and_then(|arg| arg.as_expression())
        }
        Expression::StaticMemberExpression(member) => expect_operand(&member.object),
        _ => None,
    }
}

/// Whether the comparator-less `.sort()` at `call` faces another comparator-less
/// `.sort()` across a deep-equality assertion.
///
/// Both operands then go through the *same* default comparator, so whatever
/// order it produces it produces identically on both sides: the sort is a
/// canonical form for a structural comparison of two collections whose order is
/// unspecified, not a claim about ordering, and the lexicographic surprise this
/// rule warns about cannot change the assertion's outcome. The pairing itself is
/// the evidence, so no test-path heuristic is involved, and it holds for both
/// operands — each one sees the other.
fn paired_across_equality_assertion<'a>(
    call: &CallExpression<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    semantic.nodes().ancestors(node.id()).any(|ancestor| {
        let AstKind::CallExpression(matcher) = ancestor.kind() else {
            return false;
        };
        let Expression::StaticMemberExpression(callee) = &matcher.callee else {
            return false;
        };
        if !DEEP_EQUALITY_MATCHERS.contains(&callee.property.name.as_str()) {
            return false;
        }
        let (Some(actual), Some(expected)) = (
            expect_operand(&callee.object),
            matcher.arguments.first().and_then(|arg| arg.as_expression()),
        ) else {
            return false;
        };
        let opposite = if actual.span().contains_inclusive(call.span) {
            expected
        } else if expected.span().contains_inclusive(call.span) {
            actual
        } else {
            return false;
        };
        computes_comparator_less_sort(opposite)
    })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["sort"])
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
        if !is_comparator_less_sort(call) {
            return;
        }
        // A receiver whose element type is provably `string` sorts
        // lexicographically by definition — the numeric-coercion footgun this
        // rule targets cannot occur, and the remediation it advises (`(a, b) =>
        // a - b`) does not apply.
        if expression_is_string_array(&member.object, semantic) {
            return;
        }
        if paired_across_equality_assertion(call, node, semantic) {
            return;
        }
        // `<expr>.searchParams` is the spec-defined `URL.prototype.searchParams`
        // getter, returning a `URLSearchParams`, whose `.sort()` is a distinct
        // built-in that takes no comparator — it sorts key/value pairs in place by
        // key. It is not `Array.prototype.sort`, so the numeric-coercion footgun
        // cannot occur and passing a comparator would be a type error.
        if let Expression::StaticMemberExpression(inner) = &member.object
            && inner.property.name.as_str() == "searchParams"
        {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.sort()` without comparator sorts lexicographically — pass an explicit compare function.".into(),
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
    fn flags_empty_sort() {
        assert_eq!(run_on("const sorted = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_sort_with_whitespace() {
        assert_eq!(run_on("const sorted = arr.sort(  );").len(), 1);
    }

    #[test]
    fn allows_sort_with_comparator() {
        assert!(run_on("const sorted = arr.sort((a, b) => a - b);").is_empty());
    }

    #[test]
    fn allows_object_keys_sort() {
        assert!(run_on("Object.keys(x).sort();").is_empty());
    }

    #[test]
    fn allows_object_get_own_property_names_sort() {
        assert!(run_on("Object.getOwnPropertyNames(x).sort();").is_empty());
    }

    #[test]
    fn allows_object_keys_sort_chained() {
        assert!(
            run_on("Object.keys(allMigrations).sort().map((name) => name);").is_empty()
        );
    }

    #[test]
    fn flags_array_literal_sort() {
        assert_eq!(run_on("const sorted = [10, 2, 1].sort();").len(), 1);
    }

    #[test]
    fn flags_object_values_sort() {
        // `Object.values(x)` is not spec-guaranteed `string[]` (values may be
        // numbers) — the footgun applies, so it must still flag.
        assert_eq!(run_on("Object.values(x).sort();").len(), 1);
    }

    #[test]
    fn allows_url_search_params_sort() {
        // `URLSearchParams.prototype.sort()` is a distinct no-comparator built-in.
        assert!(run_on("url.searchParams.sort();").is_empty());
    }

    #[test]
    fn allows_search_params_sort_any_base_expr() {
        assert!(run_on("this.foo.searchParams.sort();").is_empty());
    }

    #[test]
    fn flags_non_search_params_member_sort() {
        // A `.<prop>.sort()` receiver whose property isn't `searchParams` is still
        // an unknown (likely array) receiver — the footgun applies.
        assert_eq!(run_on("foo.bar.sort();").len(), 1);
    }

    // --- Both operands of a deep-equality assertion sorted (#8138) ---

    #[test]
    fn allows_both_operands_sorted_in_to_equal() {
        let src = "expect(entries.sort()).toEqual(keyValues.sort());";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_both_operands_sorted_through_transforms() {
        let src = "expect(entries.map(([k, [v, d]]) => [k, d]).sort()).toEqual(terms.map(t => [t, dist(t)]).filter(([, d]) => d <= max).sort());";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_both_operands_sorted_in_to_strict_equal() {
        let src = "expect(a.sort()).toStrictEqual(b.sort());";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_both_operands_sorted_through_not_modifier() {
        let src = "expect(a.sort()).not.toEqual(b.sort());";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A literal expected value encodes an ordering claim, so the single sorted
    // operand is still asserting the order the default comparator produced.
    #[test]
    fn flags_sort_against_literal_expectation() {
        assert_eq!(run_on("expect(a.sort()).toEqual([1, 2, 10]);").len(), 1);
    }

    // The opposite operand carries a comparator, so it is not the same
    // canonicalisation applied twice.
    #[test]
    fn flags_bare_sort_facing_compared_sort() {
        let src = "expect(a.sort((x, y) => x - y)).toEqual(b.sort());";
        assert_eq!(run_on(src).len(), 1);
    }

    // `toBe` compares references, so sorting proves nothing about the outcome.
    #[test]
    fn flags_both_operands_sorted_in_to_be() {
        assert_eq!(run_on("expect(a.sort()).toBe(b.sort());").len(), 2);
    }

    #[test]
    fn flags_sort_outside_any_assertion() {
        assert_eq!(run_on("const out = items.sort(); use(out);").len(), 1);
    }

    // --- Receiver proven `string[]` (#6356) ---

    #[test]
    fn allows_sort_of_annotated_string_array_binding() {
        let src = "const files: string[] = load(); files.sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_generic_string_array_binding() {
        let src = "const tags: Array<string> = load(); tags.sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_string_array_parameter() {
        let src = "function render(names: string[]) { return names.sort(); }";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_string_literal_array_binding() {
        let src = "const order = ['b', 'a']; order.sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_string_array_assertion() {
        let src = "(load() as string[]).sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_readonly_string_array_spread_copy() {
        let src = "const tags: readonly string[] = load(); [...tags].sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_filtered_object_keys() {
        let src = "Object.keys(o).filter(Boolean).sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sort_of_binding_initialised_from_object_keys() {
        let src = "const names = Object.keys(o); names.sort();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // A `map` callback returns whatever it likes, so the element type of its
    // result is not the receiver's.
    #[test]
    fn flags_sort_of_mapped_object_keys() {
        assert_eq!(run_on("Object.keys(o).map(f).sort();").len(), 1);
    }

    #[test]
    fn flags_sort_of_number_array_binding() {
        assert_eq!(run_on("const ids: number[] = load(); ids.sort();").len(), 1);
    }

    // The receiver's NAME is never evidence: an unresolved `files` proves
    // nothing about its element type.
    #[test]
    fn flags_sort_of_unresolved_receiver() {
        assert_eq!(run_on("files.sort();").len(), 1);
    }

    // A binding may legally be initialised from itself; resolution must
    // terminate rather than recurse forever.
    #[test]
    fn flags_sort_of_self_initialised_binding() {
        assert_eq!(run_on("var xs = xs; xs.sort();").len(), 1);
    }
}
