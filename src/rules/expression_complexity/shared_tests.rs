//! Cross-backend scenarios for `expression-complexity` — cases whose verdict
//! must be the same in Rust and in TypeScript.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_rs(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::rust::Check, src, "t.rs")
}

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule_by_id(super::META.id, src, "t.ts")
}

/// Regression for rbaumier/comply#8114 — the chain reaches the threshold, but
/// every operand is a name already.
#[test]
fn chain_of_named_operands_not_flagged() {
    let rs = "fn f(a: bool, b: bool, c: bool, d: bool, e: bool) -> bool { a && b && c && d && e }";
    assert!(run_rs(rs).is_empty());
    let ts = "const ok = a && b && c && d && e;";
    assert!(run_ts(ts).is_empty());
}

#[test]
fn chain_with_one_call_operand_flagged() {
    let rs = "fn f() { let ok = a && b && c && d && e.ready(); }";
    assert_eq!(run_rs(rs).len(), 1);
    let ts = "const ok = a && b && c && d && e.ready();";
    assert_eq!(run_ts(ts).len(), 1);
}

#[test]
fn negated_names_are_still_names() {
    let rs = "fn f() { let ok = !a && !b && !c && !d && !e; }";
    assert!(run_rs(rs).is_empty());
    let ts = "const ok = !a && !b && !c && !d && !e;";
    assert!(run_ts(ts).is_empty());
}

#[test]
fn threshold_boundary_is_the_same_in_both_backends() {
    let rs_at = "fn f() { let ok = a.p() && b.q() && c.r() && d.s() && e.t(); }";
    assert_eq!(run_rs(rs_at).len(), 1);
    let ts_at = "const ok = a.p() && b.q() && c.r() && d.s() && e.t();";
    assert_eq!(run_ts(ts_at).len(), 1);

    let rs_below = "fn f() { let ok = a.p() && b.q() && c.r() && d.s(); }";
    assert!(run_rs(rs_below).is_empty());
    let ts_below = "const ok = a.p() && b.q() && c.r() && d.s();";
    assert!(run_ts(ts_below).is_empty());
}

#[test]
fn operators_inside_a_string_literal_not_counted() {
    let rs = "fn f() -> &'static str { \"a && b && c && d && e\" }";
    assert!(run_rs(rs).is_empty());
    let ts = "const s = \"a && b && c && d && e\";";
    assert!(run_ts(ts).is_empty());
}
