//! redundant-logical-operand tests — ported from Biome's
//! `useSimplifiedLogicExpression` valid/invalid fixtures, minus the De Morgan
//! cases which comply covers (in the opposite direction) via
//! `de-morgan-simplify`.

use super::oxc_typescript::Check;
use crate::diagnostic::Diagnostic;
use crate::rules::test_helpers::run_rule;

fn run_on(source: &str) -> Vec<Diagnostic> {
    run_rule(&Check, source, "t.ts")
}

// --- Biome invalid.js fixtures (boolean-literal / null cases only) ---

#[test]
fn flags_true_and_x() {
    // Biome: `const r = true && boolExp;`
    assert_eq!(run_on("const r = true && boolExp;").len(), 1);
}

#[test]
fn flags_x_or_true() {
    // Biome: `const r2 = boolExp || true;`
    assert_eq!(run_on("const r2 = boolExp || true;").len(), 1);
}

#[test]
fn flags_null_coalesce_x() {
    // Biome: `const r3 = null ?? nonNullExp;`
    assert_eq!(run_on("const r3 = null ?? nonNullExp;").len(), 1);
}

// --- Remaining boolean-literal short-circuit arms Biome's run() covers ---

#[test]
fn flags_false_and_x() {
    assert_eq!(run_on("const r = false && x;").len(), 1);
}

#[test]
fn flags_x_and_true() {
    // `x && true` is `x` only when `x` is provably boolean (#3741).
    assert_eq!(run_on("const r = (a === b) && true;").len(), 1);
}

#[test]
fn flags_x_and_false() {
    assert_eq!(run_on("const r = x && false;").len(), 1);
}

#[test]
fn flags_true_or_x() {
    assert_eq!(run_on("const r = true || x;").len(), 1);
}

#[test]
fn flags_false_or_x() {
    assert_eq!(run_on("const r = false || x;").len(), 1);
}

#[test]
fn flags_x_or_false() {
    // `x || false` is `x` only when `x` is provably boolean (#3741).
    assert_eq!(run_on("const r = (a === b) || false;").len(), 1);
}

// --- Biome valid.js fixtures (no diagnostic) ---

#[test]
fn allows_bare_true_literal() {
    // Biome: `const boolExpr3 = true;`
    assert!(run_on("const boolExpr3 = true;").is_empty());
}

#[test]
fn allows_bare_false_literal() {
    // Biome: `const boolExpr4 = false;`
    assert!(run_on("const boolExpr4 = false;").is_empty());
}

#[test]
fn allows_de_morgan_negated_and() {
    // Biome: `const r5 = !(boolExpr1 && boolExpr2);`
    // Owned by de-morgan-simplify; not a literal short-circuit.
    assert!(run_on("const r5 = !(boolExpr1 && boolExpr2);").is_empty());
}

#[test]
fn allows_double_negation_or() {
    // Biome: `const r6 = !!boolExpr1 || !!boolExpr2;`
    assert!(run_on("const r6 = !!boolExpr1 || !!boolExpr2;").is_empty());
}

#[test]
fn allows_double_negation_statement() {
    // Biome: `!!x;`
    assert!(run_on("!!x;").is_empty());
}

// --- Disjointness / no double-report guards ---

#[test]
fn allows_de_morgan_input_owned_by_other_rule() {
    // `!a || !b` is Biome's De Morgan case; comply covers the inverse
    // (`!(a && b)`) in de-morgan-simplify, so this rule stays silent here.
    assert!(run_on("const r = !a || !b;").is_empty());
}

#[test]
fn allows_plain_logical_without_literal() {
    assert!(run_on("const r = a && b;").is_empty());
    assert!(run_on("const r = a || b;").is_empty());
    assert!(run_on("const r = a ?? b;").is_empty());
}

#[test]
fn allows_non_null_left_coalesce() {
    // `??` only simplifies when the LEFT operand is the null literal.
    assert!(run_on("const r = x ?? null;").is_empty());
    assert!(run_on("const r = x ?? false;").is_empty());
}

// --- #3741: `x || false` / `x && true` only redundant when `x` is provably
// boolean by shape; `||`/`&&` return an operand, not a coerced boolean, so on a
// `boolean | undefined` operand the literal is a meaningful coercion to `boolean`.

#[test]
fn allows_or_false_on_bare_identifier() {
    // `value || false` where `value: boolean | undefined` coerces to `boolean`.
    assert!(run_on("const r = value || false;").is_empty());
}

#[test]
fn allows_or_false_on_optional_chained_member() {
    assert!(run_on("const r = obj?.flag || false;").is_empty());
}

#[test]
fn allows_or_false_on_plain_member() {
    assert!(run_on("const r = obj.flag || false;").is_empty());
}

#[test]
fn allows_or_false_on_unknown_call() {
    assert!(run_on("const r = getThing() || false;").is_empty());
}

#[test]
fn allows_and_true_on_bare_identifier() {
    assert!(run_on("const r = x && true;").is_empty());
}

#[test]
fn flags_or_false_on_comparison() {
    assert_eq!(run_on("const r = (a === b) || false;").len(), 1);
}

#[test]
fn flags_or_false_on_boolean_builtin() {
    assert_eq!(run_on("const r = list.includes(x) || false;").len(), 1);
}

#[test]
fn flags_or_false_on_unary_not() {
    assert_eq!(run_on("const r = !ready || false;").len(), 1);
}

#[test]
fn flags_and_true_on_comparison() {
    assert_eq!(run_on("const r = (a === b) && true;").len(), 1);
}
