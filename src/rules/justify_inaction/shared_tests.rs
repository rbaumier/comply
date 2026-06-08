//! Cross-backend scenarios for `justify-inaction`.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_rs(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rust(src, &super::rust::Check)
}

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_oxc_ts(src, &super::oxc_typescript::Check)
}

#[test]
fn empty_else_flagged_cross_backend() {
    let rs = "fn f(x: bool) { if x { go(); } else {} }";
    let ts = "if (x) { go(); } else {}";
    assert_eq!(run_rs(rs).len(), 1);
    assert_eq!(run_ts(ts).len(), 1);
}

#[test]
fn commented_else_not_flagged_cross_backend() {
    let rs = "fn f(x: bool) { if x { go(); } else { /* no-op */ } }";
    let ts = "if (x) { go(); } else { /* no-op */ }";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}

#[test]
fn empty_loop_flagged_cross_backend() {
    let rs = "fn f() { while poll() {} }";
    let ts = "while (poll()) {}";
    assert_eq!(run_rs(rs).len(), 1);
    assert_eq!(run_ts(ts).len(), 1);
}

#[test]
fn empty_stub_callable_body_ignored_cross_backend() {
    let rs = "fn stub() {}";
    let ts = "function stub() {}";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}
