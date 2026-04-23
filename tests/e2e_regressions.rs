//! E2E regression tests — one per round of fixes, locking in the fix.

mod common;

use assert_cmd::Command;
use common::write_ts_file;
use predicates::prelude::*;

#[test]
fn marker_inside_string_literal_is_not_honored() {
    // Round 3: hardened marker matching to require leading whitespace only.
    // String literals containing "// comply-ignore: ..." must NOT register
    // a phantom suppression that swallows the next line.
    let source = "const fake = \"// comply-ignore: no-nested-ternary — bypass\";\nexport const x = a ? b ? 1 : 2 : 3;\n";
    let (_dir, path) = write_ts_file("phantom.ts", source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("no-nested-ternary"));
}

#[test]
fn parse_errors_do_not_emit_phantom_diagnostics() {
    // Round 2: walker skips ERROR/MISSING subtrees so a truncated function
    // body doesn't emit a max-function-lines diagnostic on recovered junk.
    let source = "function f() { const x =\n";
    let (_dir, path) = write_ts_file("broken.ts", source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("max-function-lines").not());
}

#[test]
fn jsx_files_use_tsx_grammar() {
    // Round 2: split Language::Tsx so .jsx/.tsx use LANGUAGE_TSX. Without
    // this, JSX expressions parse as ERROR nodes — either missing real
    // violations or emitting phantoms.
    let source = "export const App = () => <div onClick={() => { const x = a ? b ? 1 : 2 : 3; }}>x</div>;\n";
    let (_dir, path) = write_ts_file("app.jsx", source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("no-nested-ternary"));
}

#[test]
fn banned_identifiers_does_not_flag_document_or_database() {
    // Round 1: added word-boundary check so "document"/"database"/"domain"
    // are not flagged for starting with "do".
    let source = "const document = {}; const database = {}; const domain = '';\n";
    let (_dir, path) = write_ts_file("words.ts", source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("no-generic-names").not());
}

#[test]
fn trailing_comply_ignore_suppresses_current_line() {
    // Round 5: same-line trailing markers suppress the current line.
    let source = "export const x = a ? b ? 1 : 2 : 3; // comply-ignore: no-nested-ternary — boundary\n";
    let (_dir, path) = write_ts_file("trailing.ts", source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("no-nested-ternary").not());
}

#[test]
fn bom_prefixed_file_honors_line_one_ignore() {
    // Round 4: strip leading UTF-8 BOM before scanning ignore markers,
    // otherwise line-1 ignores silently never apply.
    let source = "\u{FEFF}// comply-ignore: no-nested-ternary — startup boundary\nexport const x = a ? b ? 1 : 2 : 3;\n";
    let (_dir, path) = write_ts_file("bom.ts", source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("no-nested-ternary").not());
}
