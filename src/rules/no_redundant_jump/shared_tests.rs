//! Cross-backend scenarios for `no-redundant-jump`.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_rs(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::rust::Check, src, "t.rs")
}

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::typescript::Check, src, "t.ts")
}

#[test]
fn trailing_return_flagged() {
    let rs = "fn f() { go(); return; }";
    let ts = "function f() { go(); return; }";
    assert_eq!(run_rs(rs).len(), 1);
    assert_eq!(run_ts(ts).len(), 1);
}

#[test]
fn early_exit_return_not_flagged() {
    let rs = "fn f(x: bool) { if x { return; } bar(); }";
    let ts = "function f(x) { if (x) { return; } bar(); }";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}

#[test]
fn trailing_continue_in_loop_flagged() {
    let rs = "fn f(xs: &[i32]) { for x in xs { go(); continue; } }";
    let ts = "function f(xs) { for (const x of xs) { go(); continue; } }";
    assert_eq!(run_rs(rs).len(), 1);
    assert_eq!(run_ts(ts).len(), 1);
}

#[test]
fn early_exit_continue_in_loop_not_flagged() {
    let rs = "fn f(xs: &[i32]) { for x in xs { if *x < 0 { continue; } go(); } }";
    let ts = "function f(xs) { for (const x of xs) { if (x < 0) { continue; } go(); } }";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}

#[test]
fn return_with_value_not_flagged() {
    let rs = "fn f() -> i32 { go(); return 42; }";
    let ts = "function f() { go(); return 42; }";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}
