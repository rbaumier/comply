//! Cross-backend scenarios for `justify-inaction`.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_rs(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::rust::Check, src, "t.rs")
}

#[test]
fn empty_else_flagged_cross_backend() {
    let rs = "fn f(x: bool) { if x { go(); } else {} }";
    assert_eq!(run_rs(rs).len(), 1);
}

#[test]
fn commented_else_not_flagged_cross_backend() {
    let rs = "fn f(x: bool) { if x { go(); } else { /* no-op */ } }";
    assert!(run_rs(rs).is_empty());
}

#[test]
fn empty_loop_flagged_cross_backend() {
    // A bare-flag condition has no call, so both backends flag it. (A
    // call-condition `while poll() {}` is exempt in the Rust backend per the
    // embedded register-polling idiom — see rust::tests, issue #1436.)
    let rs = "fn f(running: bool) { while running {} }";
    assert_eq!(run_rs(rs).len(), 1);
}

#[test]
fn empty_stub_callable_body_ignored_cross_backend() {
    let rs = "fn stub() {}";
    assert!(run_rs(rs).is_empty());
}
