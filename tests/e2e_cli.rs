//! E2E tests for CLI behavior — exit codes, help, suppressions, output format.

mod common;

use common::write_ts_file;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn exit_code_zero_on_clean_file() {
    // Minimal clean file — suppress dead-export since this is an isolated test file.
    let (_dir, path) = write_ts_file(
        "clean.ts",
        "// comply-ignore: dead-export — isolated test file\n\
         export const GREETING = \"hello\";\n",
    );
    // Colocated test file so the colocated-tests rule doesn't fire.
    std::fs::write(
        path.with_file_name("clean.test.ts"),
        "test('ok', () => {});\n",
    )
    .unwrap();
    common::comply()
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
    common::comply()
        .arg(&path)
        .assert()
        .code(1);
}

#[test]
fn comply_ignore_suppresses_diagnostic() {
    let source = "// comply-ignore: no-throw — legacy migration path\nfunction f() { throw new Error('x'); }\n";
    let (_dir, path) = write_ts_file("ignored.ts", source);
    common::comply()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("no-throw").not());
}

#[test]
fn comply_ignore_without_justification_is_flagged() {
    let source = "// comply-ignore: no-throw\nfunction f() { return 1; }\n";
    let (_dir, path) = write_ts_file("bad_ignore.ts", source);
    common::comply()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains(
            "comply-ignore-missing-justification",
        ));
}

#[test]
fn help_flag_prints_usage() {
    common::comply()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Your code will comply"));
}

#[test]
fn unknown_extension_is_skipped_silently() {
    let (_dir, path) = write_ts_file("file.txt", "throw new Error('x');\n");
    common::comply()
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("no files to lint"));
}

#[test]
fn empty_directory_returns_clean() {
    let dir = TempDir::new().unwrap();
    common::comply()
        .arg(dir.path())
        .assert()
        .success();
}

// Regression for #5941: type-aware is on by default, but a project with no
// tsconfig.json has nothing to type-check — that must stay a graceful skip,
// never a hard failure. Only an explicit toolchain-config gap (missing node /
// @typescript/native-preview) fails loud.
//
// The graceful no-tsconfig skip is only reached after the node-availability
// check passes, so the contract has a node precondition: with `node` on PATH
// the run prints the graceful no-tsconfig skip; without it, fail-loud
// correctly bails with a message that names the node gap. Asserting on either
// stderr message keeps the test robust to node-absence while still surfacing a
// real regression (a no-tsconfig run failing for some *other* reason).
#[test]
fn no_tsconfig_is_refused_not_silently_skipped() {
    // The type-aware rules need a resolved TypeScript program, which needs a
    // tsconfig.json. The phase used to skip itself on a stderr notice and the
    // run still ended on "all clear" and exit 0 — a verdict on code those rules
    // never saw. It is a refusal now, with the two ways out of it. The fixture
    // is built here rather than through `write_ts_file`, whose comply.toml
    // turns those rules off.
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join("no_tsconfig.ts");
    std::fs::write(
        &path,
        "// comply-ignore: dead-export — isolated test file\n\
         export const GREETING = \"hello\";\n",
    )
    .unwrap();
    std::fs::write(
        path.with_file_name("no_tsconfig.test.ts"),
        "test('ok', () => {});\n",
    )
    .unwrap();
    common::comply()
        .arg(&path)
        .assert()
        .code(2)
        .stdout(predicate::str::contains("all clear").not())
        .stderr(
            predicate::str::contains("no tsconfig.json")
                .or(predicate::str::contains("needs Node.js on PATH")),
        );
}

#[test]
fn output_format_matches_eslint_pattern() {
    let (_dir, path) = write_ts_file("err.ts", "function f() { throw 1; }\n");
    common::comply()
        .arg(&path)
        .assert()
        .stdout(predicate::str::is_match(r":\d+:\d+: (error|warning) \[").unwrap());
}
