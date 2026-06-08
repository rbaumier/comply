//! Cross-backend scenarios for `no-timing-attack`.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_rs(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::rust::Check, src, "t.rs")
}

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::typescript::Check, src, "t.ts")
}

#[test]
fn direct_password_comparison_flagged() {
    let rs = "fn f(password: &str, input: &str) -> bool { password == input }";
    let ts = "function f(password, input) { return password === input; }";
    assert_eq!(run_rs(rs).len(), 1);
    assert_eq!(run_ts(ts).len(), 1);
}

#[test]
fn token_type_lexer_not_flagged() {
    let rs = "fn f() -> bool { token_type == other_type }";
    let ts = "function f() { return tokenType === otherType; }";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}

#[test]
fn string_literal_containing_sensitive_word_not_flagged() {
    // The original walkthrough FP: a node-kind string containing
    // "signature" as a substring.
    let rs = r#"fn f(n: tree_sitter::Node) { let _ = n.kind() != "index_signature"; }"#;
    let ts = r#"function f(node) { return node.type !== "index_signature"; }"#;
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}

#[test]
fn unrelated_name_comparison_not_flagged() {
    let rs = "fn f() -> bool { name == other }";
    let ts = "function f() { return name === other; }";
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}

#[test]
fn member_access_sensitive_flagged() {
    let rs = "fn f(user: &User, input: &str) -> bool { user.password_hash == input }";
    let ts = "function f(user, input) { return user.password === input; }";
    assert_eq!(run_rs(rs).len(), 1);
    assert_eq!(run_ts(ts).len(), 1);
}
