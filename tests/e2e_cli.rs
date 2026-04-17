//! E2E tests for CLI behavior — exit codes, help, suppressions, output format.

mod common;

use assert_cmd::Command;
use common::write_ts_file;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn exit_code_zero_on_clean_file() {
    // No exports, a const used by `console.log` (satisfies
    // `no-unused-vars`), and a module-level JSDoc with prose + `@file` tag
    // (satisfies `jsdoc-require-file-overview` and `jsdoc-needs-description`).
    let (_dir, path) = write_ts_file(
        "clean.ts",
        "/**\n\
         \x20* Sample clean file for the comply exit-code test.\n\
         \x20*\n\
         \x20* @file Sample clean file for the comply exit-code test.\n\
         \x20*/\n\
         \n\
         const GREETING = \"hello\";\n\
         console.log(GREETING);\n",
    );
    // Colocated test file so the colocated-tests rule doesn't fire.
    std::fs::write(path.with_file_name("clean.test.ts"), "test('ok', () => {});\n").unwrap();
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
fn output_format_matches_eslint_pattern() {
    let (_dir, path) = write_ts_file("err.ts", "function f() { throw 1; }\n");
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::is_match(r":\d+:\d+: (error|warning) \[").unwrap());
}
