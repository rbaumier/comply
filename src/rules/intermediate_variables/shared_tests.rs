//! Cross-backend scenarios for `intermediate-variables`. TS-only cases
//! live here too: the oxc backend has no test module of its own.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_rs(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::rust::Check, src, "t.rs")
}

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule_by_id(super::META.id, src, "t.ts")
}

#[test]
fn three_operand_chain_flagged() {
    let rs = "fn f() { if a.open() && b.len() > 0 && c.ready { go(); } }";
    assert_eq!(run_rs(rs).len(), 1);
    let ts = "if (a.open() && b.length > 0 && c.ready) { go(); }";
    assert_eq!(run_ts(ts).len(), 1);
}

#[test]
fn chain_of_named_operands_not_flagged() {
    let rs = "fn f() { if (a || b) && !c { go(); } }";
    assert!(run_rs(rs).is_empty());
    let ts = "if ((a || b) && !c) { go(); }";
    assert!(run_ts(ts).is_empty());
}

#[test]
fn chain_with_one_unnamed_operand_flagged() {
    let rs = "fn f() { if (a || b) && !c.is_terminated() { go(); } }";
    assert_eq!(run_rs(rs).len(), 1);
    let ts = "if ((a || b) && !c.isTerminated()) { go(); }";
    assert_eq!(run_ts(ts).len(), 1);
}

#[test]
fn two_operand_chain_not_flagged() {
    let rs = "fn f() { if a && b { go(); } }";
    assert!(run_rs(rs).is_empty());
}

#[test]
fn ts_only_nullish_chain_of_named_operands_not_flagged() {
    // `??` counts as a logical op, so this chain reaches the threshold;
    // its operands are names, so there is nothing to extract.
    assert!(run_ts("if (a ?? b ?? c) { go(); }").is_empty());
}

#[test]
fn ts_only_typeof_operand_is_unnamed() {
    // `typeof c` yields a string, not a named boolean.
    assert_eq!(run_ts("if ((a || b) && typeof c) { go(); }").len(), 1);
}

#[test]
fn comparison_ops_not_counted() {
    // `!=` / `!==` and arithmetic are not logical ops.
    let rs = r#"fn f() { if !ok() && code() != Some(1) { go(); } }"#;
    assert!(run_rs(rs).is_empty());
}

#[test]
fn plain_call_with_complex_args_not_flagged() {
    // The rule no longer looks at call_expression at all; complex
    // argument expressions are fine.
    let rs = "fn f() { do_stuff(a + b * c / d); }";
    assert!(run_rs(rs).is_empty());
}

#[test]
fn callable_boundary_blocks_count_propagation() {
    // A lambda passed as argument to a call inside the if condition
    // has its own operator count. Outer `if` sees 0 logical ops.
    let rs = "fn f(items: &[Item]) { if items.iter().any(|x| x.a && x.b && x.c) { go(); } }";
    assert!(run_rs(rs).is_empty());
}
