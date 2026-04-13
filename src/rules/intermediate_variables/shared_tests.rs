//! Cross-backend scenarios for `intermediate-variables`.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_rs(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rust(src, &super::rust::Check)
}

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_ts(src, &super::typescript::Check)
}

#[test]
fn three_operand_chain_flagged() {
    let rs = "fn f() { if a && b && c { go(); } }";
    let ts = "if (a && b && c) { go(); }";
    assert_eq!(run_rs(rs).len(), 1);
    assert_eq!(run_ts(ts).len(), 1);
}

#[test]
fn two_operand_chain_not_flagged() {
    let rs = "fn f() { if a && b { go(); } }";
    let ts = "if (a && b) { go(); }";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}

#[test]
fn comparison_ops_not_counted() {
    // `!=` / `!==` and arithmetic are not logical ops.
    let rs = r#"fn f() { if !ok() && code() != Some(1) { go(); } }"#;
    let ts = "if (!ok() && code() !== 1) { go(); }";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}

#[test]
fn plain_call_with_complex_args_not_flagged() {
    // The rule no longer looks at call_expression at all; complex
    // argument expressions are fine.
    let rs = "fn f() { do_stuff(a + b * c / d); }";
    let ts = "doStuff(a + b * c / d);";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}

#[test]
fn callable_boundary_blocks_count_propagation() {
    // A lambda passed as argument to a call inside the if condition
    // has its own operator count. Outer `if` sees 0 logical ops.
    let rs = "fn f(items: &[Item]) { if items.iter().any(|x| x.a && x.b && x.c) { go(); } }";
    let ts = "if (items.some(x => x.a && x.b && x.c)) { go(); }";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}
