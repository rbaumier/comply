//! E2E regression tests — one per round of fixes, locking in the fix.

mod common;

use assert_cmd::Command;
use common::write_ts_file;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

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
    let source =
        "export const App = () => <div onClick={() => { const x = a ? b ? 1 : 2 : 3; }}>x</div>;\n";
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
    let source =
        "export const x = a ? b ? 1 : 2 : 3; // comply-ignore: no-nested-ternary — boundary\n";
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

#[test]
fn comply_ignore_above_jsdoc_suppresses_function_below() {
    // Regression for rbaumier/comply#185 — a per-function comply-ignore
    // marker sitting above a JSDoc block must still suppress the rule on
    // the function declaration below the doc comment. The suppression walk
    // must skip JSDoc lines to land on the actual declaration.
    let source = "// comply-ignore: cyclomatic-complexity — exhaustive dispatch over a union.\n\
        /**\n * Authorize a caller against an intent.\n */\n\
        export function authorize(intent: { kind: string }) {\n\
            if (intent.kind === 'a') return 1;\n\
            if (intent.kind === 'b') return 2;\n\
            if (intent.kind === 'c') return 3;\n\
            if (intent.kind === 'd') return 4;\n\
            if (intent.kind === 'e') return 5;\n\
            if (intent.kind === 'f') return 6;\n\
            if (intent.kind === 'g') return 7;\n\
            if (intent.kind === 'h') return 8;\n\
            if (intent.kind === 'i') return 9;\n\
            if (intent.kind === 'j') return 10;\n\
            if (intent.kind === 'k') return 11;\n\
            return 12;\n\
        }\n";
    let (_dir, path) = write_ts_file("authorize.ts", source);
    Command::cargo_bin("comply")
        .unwrap()
        .arg(&path)
        .assert()
        .stdout(predicate::str::contains("cyclomatic-complexity").not());
}

#[test]
fn comply_ignore_file_suppresses_unused_file_on_plugin_resolved_entry() {
    // A `.tsx` file unreachable from any static entry point (e.g. a TanStack
    // Start client entry whose
    // import is emitted by the Vite plugin's boot module, not user
    // source) must be excluded from `unused-file` when it starts with
    // `// comply-ignore-file: unused-file — <reason>`. The file-level
    // directive runs in the post-pass over diagnostics keyed by path,
    // so cross-file rules honour it the same way per-file rules do.
    let dir = TempDir::new().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("src/app")).unwrap();
    fs::write(
        dir.path().join("index.ts"),
        "export const ENTRY = 1;\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/app/client.tsx"),
        "// comply-ignore-file: unused-file — Vite plugin emits the import\n\
         import { hydrateRoot } from \"react-dom/client\";\n\
         export const HYDRATE = hydrateRoot;\n",
    )
    .unwrap();

    Command::cargo_bin("comply")
        .unwrap()
        .arg(dir.path())
        .assert()
        .stdout(predicate::str::contains("unused-file").not());
}
