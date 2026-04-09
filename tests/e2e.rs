//! End-to-end tests — invoke the comply binary on fixture files and assert
//! on stdout, stderr, and exit codes.
//!
//! These tests build the binary via `cargo run` (assert_cmd handles that)
//! and run it as a subprocess, mimicking real CLI usage.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Helper — write a TS file in a temp dir and return the dir + path.
fn write_ts_file(name: &str, content: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join(name);
    fs::write(&path, content).expect("failed to write fixture");
    (dir, path)
}

#[test]
fn exit_code_zero_on_clean_file() {
    let (_dir, path) = write_ts_file("clean.ts", "const x = 5;\n");
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .success();
}

#[test]
fn exit_code_one_on_violations() {
    let (_dir, path) = write_ts_file(
        "bad.ts",
        "function handleClick() { throw new Error('boom'); }\n",
    );
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .code(1);
}

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
fn comply_ignore_suppresses_diagnostic() {
    let source = "// comply-ignore: no-throw — legacy migration path\nfunction f() { throw new Error('x'); }\n";
    let (_dir, path) = write_ts_file("ignored.ts", source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("no-throw").not());
}

#[test]
fn comply_ignore_without_justification_is_flagged() {
    let source = "// comply-ignore: no-throw\nfunction f() { return 1; }\n";
    let (_dir, path) = write_ts_file("bad_ignore.ts", source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("comply-ignore-missing-justification"));
}

#[test]
fn help_flag_prints_usage() {
    Command::cargo_bin("comply")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Your code will comply"));
}

#[test]
fn unknown_extension_is_skipped_silently() {
    let (_dir, path) = write_ts_file("file.txt", "throw new Error('x');\n");
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("no files to lint"));
}

#[test]
fn empty_directory_returns_clean() {
    let dir = TempDir::new().unwrap();
    Command::cargo_bin("comply")
        .unwrap()
        .arg(dir.path())
        .assert()
        .success();
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

#[test]
fn output_format_matches_eslint_pattern() {
    let (_dir, path) = write_ts_file("err.ts", "function f() { throw 1; }\n");
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::is_match(r":\d+:\d+: (error|warning) \[").unwrap());
}
