//! Cross-backend scenarios for `no-duplicated-branches`.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_rs(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::rust::Check, src, "t.rs")
}

#[test]
fn simple_if_else_duplicate_flagged_once() {
    let rs = r#"fn f(a: bool) { if a { go(); } else { go(); } }"#;
    assert_eq!(run_rs(rs).len(), 1);
}

#[test]
fn three_identical_branches_dedup_to_two() {
    let rs = r#"fn f(a: bool, b: bool) {
    if a { go(); }
    else if b { go(); }
    else { go(); }
}"#;
    assert_eq!(run_rs(rs).len(), 2);
}

#[test]
fn distinct_branches_not_flagged() {
    let rs = r#"fn f(a: bool) { if a { foo(); } else { bar(); } }"#;
    assert!(run_rs(rs).is_empty());
}
