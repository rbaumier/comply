//! E2E tests for each lint rule's detection logic.

mod common;

use assert_cmd::Command;
use common::write_ts_file;
use predicates::prelude::*;

#[test]
fn detects_throw_statement() {
    let (_dir, path) = write_ts_file("throw.ts", "function f() { throw new Error('x'); }\n");
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("no-throw"));
}

#[test]
fn detects_banned_identifier() {
    let (_dir, path) = write_ts_file("banned.ts", "function processOrder() { return 1; }\n");
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("banned-identifiers"))
        .stdout(predicate::str::contains("processOrder"));
}

#[test]
fn detects_nested_ternary() {
    let (_dir, path) = write_ts_file("ternary.ts", "const x = a ? b ? 1 : 2 : 3;\n");
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("no-nested-ternary"));
}

#[test]
fn detects_max_file_lines() {
    let content = "const x = 0;\n".repeat(250);
    let (_dir, path) = write_ts_file("big.ts", &content);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("max-file-lines"));
}

#[test]
fn detects_max_function_lines() {
    let body = "let x = 0;\n".repeat(35);
    let source = format!("function long() {{\n{body}}}\n");
    let (_dir, path) = write_ts_file("long_fn.ts", &source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("max-function-lines"));
}

#[test]
fn multiple_violations_are_all_reported() {
    let source = "function handleData() { throw new Error('x'); const y = a ? b ? 1 : 2 : 3; }\n";
    let (_dir, path) = write_ts_file("multi.ts", source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("no-throw"))
        .stdout(predicate::str::contains("no-nested-ternary"))
        .stdout(predicate::str::contains("banned-identifiers"));
}
