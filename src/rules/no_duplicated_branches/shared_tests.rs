//! Cross-backend scenarios for `no-duplicated-branches`.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_rs(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rust(src, &super::rust::Check)
}

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_ts(src, &super::typescript::Check)
}

#[test]
fn simple_if_else_duplicate_flagged_once() {
    let rs = r#"fn f(a: bool) { if a { go(); } else { go(); } }"#;
    let ts = r#"function f(a) { if (a) { go(); } else { go(); } }"#;
    assert_eq!(run_rs(rs).len(), 1);
    assert_eq!(run_ts(ts).len(), 1);
}

#[test]
fn three_identical_branches_dedup_to_two() {
    let rs = r#"fn f(a: bool, b: bool) {
    if a { go(); }
    else if b { go(); }
    else { go(); }
}"#;
    let ts = r#"function f(a, b) {
    if (a) { go(); }
    else if (b) { go(); }
    else { go(); }
}"#;
    assert_eq!(run_rs(rs).len(), 2);
    assert_eq!(run_ts(ts).len(), 2);
}

#[test]
fn distinct_branches_not_flagged() {
    let rs = r#"fn f(a: bool) { if a { foo(); } else { bar(); } }"#;
    let ts = r#"function f(a) { if (a) { foo(); } else { bar(); } }"#;
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}
