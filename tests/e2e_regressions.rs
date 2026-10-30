//! E2E regression tests — one per round of fixes, locking in the fix.

mod common;

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
    common::comply()
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
    common::comply()
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
    common::comply()
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
    common::comply()
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
    common::comply()
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
    common::comply()
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
    common::comply()
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

    common::comply()
        .arg(dir.path())
        .assert()
        .stdout(predicate::str::contains("unused-file").not());
}

#[test]
fn migration_lock_timeout_ignores_framework_package_dirs() {
    // node-pg-migrate / db-migrate own source: the package directory name
    // contains `migrate` as a substring but is not a migrations directory.
    // The framework's own DDL builders must not be flagged for a missing
    // `SET lock_timeout`; a real file under `migrations/` still is.
    let dir = TempDir::new().expect("failed to create temp dir");
    fs::create_dir_all(dir.path().join("node-pg-migrate/src/operations/tables"))
        .unwrap();
    fs::write(
        dir.path()
            .join("node-pg-migrate/src/operations/tables/addConstraint.ts"),
        "export const add = (tableName: string) =>\n\
         `ALTER TABLE ${tableName} ADD CONSTRAINT fk FOREIGN KEY (id);`;\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("migrations")).unwrap();
    fs::write(
        dir.path().join("migrations/001_add_age.ts"),
        "export const up = `ALTER TABLE users ADD COLUMN age INT;`;\n",
    )
    .unwrap();

    common::comply()
        .arg(dir.path())
        .assert()
        // Framework package source must NOT get the lock-timeout warning…
        .stdout(
            predicate::str::is_match(
                r"addConstraint\.ts:\d+:\d+:.*\[migration-needs-lock-timeout\]",
            )
            .unwrap()
            .not(),
        )
        // …while the real migration file still does.
        .stdout(
            predicate::str::is_match(
                r"001_add_age\.ts:\d+:\d+:.*\[migration-needs-lock-timeout\]",
            )
            .unwrap(),
        );
}

#[test]
fn sql_migration_rules_skip_test_dirs_and_host_code() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let both_defects = "ALTER TABLE t ADD CONSTRAINT t_a_chk CHECK (a > 0) NOT VALID;\n\
                        ALTER TABLE t VALIDATE CONSTRAINT t_a_chk;\n\
                        UPDATE t SET a = replace(a, 'x', 'y') WHERE id = 1;\n";
    let update = "UPDATE t SET a = replace(a, 'x', 'y') WHERE id = 1";
    for (path, content) in [
        ("migrations/002.sql", both_defects.to_string()),
        ("tests/migrations/002.sql", both_defects.to_string()),
        ("src/fix.sql", both_defects.to_string()),
        (
            "migrations/001_fix.rs",
            format!("fn up() {{ let q = \"{update}\"; }}\n"),
        ),
        (
            "migrations/001_fix.ts",
            format!("export const up = `{update}`;\n"),
        ),
    ] {
        let file = dir.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, content).unwrap();
    }

    let output = common::comply().arg(dir.path()).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let flagged: Vec<&str> = stdout
        .lines()
        .filter(|line| {
            line.contains("[sql-validate-constraint-separate-migration]")
                || line.contains("[sql-update-replace-needs-row-check]")
        })
        .collect();
    assert_eq!(flagged.len(), 2, "{stdout}");
    assert!(
        flagged
            .iter()
            .all(|line| line.contains("/migrations/002.sql") && !line.contains("/tests/")),
        "{stdout}"
    );
}

#[test]
fn vue_and_tsx_backends_of_a_rule_anchor_on_the_same_construct() {
    // One defect, one element, two file types: `a11y-alt-text` reports the
    // `<img` opening tag in both, so moving a component from `.tsx` to `.vue`
    // does not move the diagnostic to the left margin.
    let dir = TempDir::new().expect("failed to create temp dir");
    fs::write(
        dir.path().join("App.tsx"),
        "export function Gallery({ items }: { items: string[] }) {\n\
         \x20 return (\n\
         \x20   <ul>\n\
         \x20     <li>\n\
         \x20       <img src={items[0]} width=\"320\" height=\"320\" loading=\"lazy\" />\n\
         \x20     </li>\n\
         \x20   </ul>\n\
         \x20 )\n\
         }\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("App.vue"),
        "<script setup lang=\"ts\">\n\
         const items = ['a.png']\n\
         </script>\n\
         \n\
         <template>\n\
         \x20 <ul>\n\
         \x20   <li>\n\
         \x20     <img :src=\"items[0]\" width=\"320\" height=\"320\" loading=\"lazy\">\n\
         \x20   </li>\n\
         \x20 </ul>\n\
         </template>\n",
    )
    .unwrap();

    common::comply()
        .arg("rules")
        .arg("a11y-alt-text")
        .arg(dir.path())
        .assert()
        // The JSX path lands on the `<img`, at column 9.
        .stdout(predicate::str::is_match(r"App\.tsx:5:9:").unwrap())
        // The Vue path lands on its own `<img`, at column 7 — not column 1.
        .stdout(predicate::str::is_match(r"App\.vue:8:7:").unwrap());
}

#[test]
fn undeclared_dependency_reports_once_per_import_site_at_the_import() {
    // Regression for rbaumier/comply#8354, the `retejs/rete` case. One id
    // covers undeclared packages, and each diagnostic sits on the construct
    // that names the package: `rete.config.ts` imports the declared `rete-cli`
    // on line 1 and the undeclared `rollup-plugin-copy` on line 2, so the
    // report must not land on line 1. `require` and dynamic `import()` reach
    // the rule through the engine's per-node dispatch, like a static import.
    let dir = TempDir::new().expect("failed to create temp dir");
    fs::write(
        dir.path().join("package.json"),
        r#"{"name":"rete","devDependencies":{"rete-cli":"^2.1.0"}}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("rete.config.ts"),
        "import { ReteOptions } from 'rete-cli'\n\
         import copy from 'rollup-plugin-copy'\n\
         export default <ReteOptions>{ plugins: [copy()] }\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("test")).unwrap();
    fs::write(
        dir.path().join("test/index.test.ts"),
        "import { expect } from '@jest/globals'\n\
         export const check = expect\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("test/perf.ts"),
        "import { it } from '@jest/globals'\n\
         export const bench = it\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("postinstall.js"),
        "'use strict'\n\
         const shell = require('shelljs')\n\
         module.exports = () => import('open').then(() => shell)\n",
    )
    .unwrap();

    common::comply()
        .arg("rules")
        .arg("no-implicit-deps")
        .arg(dir.path())
        .assert()
        // The undeclared import, not the declared `rete-cli` on line 1.
        .stdout(predicate::str::is_match(r"rete\.config\.ts:2:1:").unwrap())
        .stdout(predicate::str::is_match(r"rete\.config\.ts:1:").unwrap().not())
        // One report per import site, each on its own file.
        .stdout(predicate::str::is_match(r"index\.test\.ts:1:1:").unwrap())
        .stdout(predicate::str::is_match(r"perf\.ts:1:1:").unwrap())
        // `require` anchors on the call, dynamic `import()` on the expression.
        .stdout(predicate::str::is_match(r"postinstall\.js:2:15:").unwrap())
        .stdout(predicate::str::is_match(r"postinstall\.js:3:24:").unwrap())
        // Five import sites, five diagnostics. Pins the granularity: a return to
        // per-package reporting, or a double-visited edge, moves this number.
        .stderr(predicate::str::contains("comply: 5 violations found"));
}

/// Run git in `dir` and fail the test with its stderr if it does not succeed.
fn git(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be on PATH");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn diff_only_survives_a_non_utf8_file_in_the_diff_issue_8502() {
    // `--staged --diff-only` is the pre-commit gate. A repository that commits
    // an ISO-8859-1 CSV on purpose (0xE9 = `é`) puts undecodable bytes in the
    // diff as soon as that file is touched; the gate must still report the
    // violation on the staged `.ts` line instead of linting nothing.
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "t"]);

    fs::write(root.join("a.ts"), "export const one = 1;\n").unwrap();
    fs::write(root.join("sample.csv"), b"name\nCaf\xe9\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "init"]);

    fs::write(
        root.join("a.ts"),
        "export const one = 1;\nexport const two = a ? b ? 1 : 2 : 3;\n",
    )
    .unwrap();
    fs::write(root.join("sample.csv"), b"name\nCaf\xe9\nTh\xe9\n").unwrap();
    git(root, &["add", "-A"]);

    common::comply()
        .current_dir(root)
        .args(["--staged", "--diff-only"])
        .assert()
        .stdout(predicate::str::contains("no-nested-ternary"))
        .stderr(predicate::str::contains("crashed unexpectedly").not());
}

#[test]
fn missing_type_aware_toolchain_is_an_instruction_not_a_crash_issue_8109() {
    // A TypeScript project with a tsconfig.json but no typescript-go API package
    // in its node_modules is every repo you clone. comply diagnoses that gap
    // itself and knows the remedy, so it must answer like `rustup component add
    // clippy` does — never "crashed unexpectedly" with a bug-report URL.
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("package.json"), "{\"name\":\"t\"}\n").unwrap();
    fs::write(
        dir.path().join("tsconfig.json"),
        "{\"compilerOptions\":{\"strict\":true},\"include\":[\"src\"]}\n",
    )
    .unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/index.ts"),
        "export const one = 1;\n",
    )
    .unwrap();

    // Whether `node` itself or the package is what's absent decides which
    // sentence comply prints; both are toolchain gaps and both must exit 2
    // through the actionable path.
    common::comply()
        .arg(dir.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("comply: type-aware analysis needs"))
        .stderr(predicate::str::contains("crashed unexpectedly").not())
        .stderr(predicate::str::contains("github.com/rbaumier/comply/issues").not());
}

/// Enables the two opt-in French copy rules.
const COPY_RULES_TOML: &str = "[rules.french-nbsp-before-punctuation]\ndisabled = false\n\
    [rules.copy-apostrophe-style]\ndisabled = false\n";

#[test]
fn fix_rewrites_each_copy_finding_in_its_context_issue_8610() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("comply.toml"), COPY_RULES_TOML).unwrap();
    fs::write(
        root.join("Page.tsx"),
        "export function Page() { return <p aria-label=\"Quoi ?\">\
         Attention : l'agent a\u{200b}b</p>; }\nexport const s = \"a\u{a0}b\";\n",
    )
    .unwrap();
    fs::write(root.join("lib.rs"), "pub const S: &str = \"a\u{a0}b\";\n").unwrap();

    common::comply()
        .arg("--fix")
        .arg(root)
        .assert()
        .stderr(predicate::str::contains(
            "comply: rewrote 6 copy finding(s)",
        ));
    let page = fs::read_to_string(root.join("Page.tsx")).unwrap();
    let lib = fs::read_to_string(root.join("lib.rs")).unwrap();
    for fixed in [
        "aria-label=\"Quoi&#8239;?\"",
        "Attention&nbsp;: l’agent a&#x200B;b",
        "\"a\\u00A0b\"",
    ] {
        assert!(page.contains(fixed), "{fixed} missing from {page}");
    }
    assert!(lib.contains("\"a\\u{a0}b\""), "{lib}");

    common::comply().arg(root).assert().stdout(
        predicate::str::contains("french-nbsp-before-punctuation")
            .or(predicate::str::contains("no-raw-invisible-character"))
            .or(predicate::str::contains("copy-apostrophe-style"))
            .not(),
    );
    common::comply()
        .arg("--fix")
        .arg(root)
        .assert()
        .stderr(predicate::str::contains(
            "comply: rewrote 0 copy finding(s)",
        ));
    assert_eq!(fs::read_to_string(root.join("Page.tsx")).unwrap(), page);
    assert_eq!(fs::read_to_string(root.join("lib.rs")).unwrap(), lib);
}

#[test]
fn fix_rewrites_copy_in_ts_strings_issue_1478() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("comply.toml"), COPY_RULES_TOML).unwrap();
    fs::write(
        root.join("lib.ts"),
        "export const m = \"Attention : l'agent répond\";\n",
    )
    .unwrap();

    common::comply()
        .arg("--fix")
        .arg(root)
        .assert()
        .stderr(predicate::str::contains(
            "comply: rewrote 2 copy finding(s)",
        ));
    let lib = fs::read_to_string(root.join("lib.ts")).unwrap();
    assert!(
        lib.contains("\"Attention\\u00A0: l’agent répond\""),
        "{lib}"
    );
    common::comply().arg(root).assert().stdout(
        predicate::str::contains("french-nbsp-before-punctuation")
            .or(predicate::str::contains("copy-apostrophe-style"))
            .not(),
    );
}

#[test]
fn staged_diff_only_fix_leaves_unchanged_lines_issue_8610() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "t"]);
    fs::create_dir(root.join("web")).unwrap();
    fs::write(root.join("comply.toml"), COPY_RULES_TOML).unwrap();
    fs::write(
        root.join("web/Page.tsx"),
        "export const A = <p>Avant : ok</p>;\n",
    )
    .unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "init"]);

    fs::write(
        root.join("web/Page.tsx"),
        "export const A = <p>Avant : ok</p>;\nexport const B = <p>Après : ok</p>;\n",
    )
    .unwrap();
    git(root, &["add", "-A"]);

    common::comply()
        .current_dir(root)
        .args(["--staged", "--diff-only", "--fix"])
        .assert()
        .stderr(predicate::str::contains(
            "comply: rewrote 1 copy finding(s)",
        ));
    let page = fs::read_to_string(root.join("web/Page.tsx")).unwrap();
    assert!(page.contains("Avant : ok"), "{page}");
    assert!(page.contains("Après&nbsp;: ok"), "{page}");
}

#[test]
fn fix_leaves_raw_literal_invisible_reported_issue_8610() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let rs = "pub const S: &str = r\"a\u{200b}b\";\n";
    let ts = "export const t = String.raw`a\u{200b}b`;\n";
    fs::write(root.join("lib.rs"), rs).unwrap();
    fs::write(root.join("a.ts"), ts).unwrap();

    common::comply()
        .arg("--fix")
        .arg(root)
        .assert()
        .stdout(predicate::str::contains("no-raw-invisible-character"));
    assert_eq!(fs::read_to_string(root.join("lib.rs")).unwrap(), rs);
    assert_eq!(fs::read_to_string(root.join("a.ts")).unwrap(), ts);
}

#[test]
fn fix_still_runs_oxlint_fixers_issue_8610() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(
        root.join("a.ts"),
        "export function f() { debugger; return 1; }\n",
    )
    .unwrap();

    common::comply()
        .arg("--fix")
        .arg(root)
        .assert()
        .stderr(predicate::str::contains("comply: ran 1 fixer(s)"));
    let ts = fs::read_to_string(root.join("a.ts")).unwrap();
    assert!(!ts.contains("debugger"), "{ts}");
}

#[test]
fn fix_leaves_file_whose_fix_breaks_parse_issue_8610() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(
        root.join("comply.toml"),
        "[rules.copy-apostrophe-style]\ndisabled = false\nstyle = \"straight\"\n",
    )
    .unwrap();
    let tsx = "export const P = () => <i title='l’agent' />;\n";
    fs::write(root.join("Page.tsx"), tsx).unwrap();

    common::comply()
        .arg("--fix")
        .arg(root)
        .assert()
        .stderr(predicate::str::contains("Page.tsx"))
        .stdout(predicate::str::contains("copy-apostrophe-style"));
    assert_eq!(fs::read_to_string(root.join("Page.tsx")).unwrap(), tsx);
}

#[test]
fn ansi_fixture_does_not_cost_its_batch_its_diagnostics_issue_8402() {
    // oxlint's json writer echoes source text into `message` unescaped, so a
    // golden fixture storing terminal output verbatim (first byte ESC) yields
    // JSON that serde_json rejects. Every `.ts` file goes into one batch of up
    // to 500, so the sibling's oxlint diagnostic is what proves the batch — and
    // with it the run — survived. "Completes" alone would pass on zero output.
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("package.json"), "{\"name\":\"t\"}\n").unwrap();
    fs::write(
        dir.path().join("ansi.ts"),
        "\u{1b}[3;38;2;248;248;242mconst x = 1;\u{1b}[0m\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("other.ts"),
        "export function f() { debugger; }\n",
    )
    .unwrap();

    common::comply()
        .arg(dir.path())
        .assert()
        .stdout(predicate::str::contains("no-debugger"))
        .stderr(predicate::str::contains("crashed unexpectedly").not());
}

/// One long exported function and one standalone comment.
/// Copied verbatim, it trips all three cross-file duplicate rules.
fn duplicated_module_8611() -> String {
    let body: String = (0..25)
        .map(|i| format!("  const value_{i} = computeExpensive(seed, \"param_{i}\");\n"))
        .collect();
    format!(
        "export function computeEverything(seed: number): number {{\n{body}  return value_0 + value_24;\n}}\n\n\
         // Retry budget reasoning: the upstream billing gateway throttles bursts above forty \
         requests, so invoices travel in windows of eight with a pause between them.\n"
    )
}

/// A git repository with `a.ts` committed, ready for staged changes.
fn repo_with_committed_original_8611() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "t"]);
    fs::write(root.join("a.ts"), duplicated_module_8611()).unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "init"]);
    dir
}

#[test]
fn staged_copy_of_an_unchanged_function_is_reported_issue_8611() {
    let dir = repo_with_committed_original_8611();
    let root = dir.path();
    fs::write(root.join("b.ts"), duplicated_module_8611()).unwrap();
    git(root, &["add", "b.ts"]);

    let on_b_naming_a = |rule: &str| {
        predicate::str::is_match(format!(r"b\.ts:\d+:\d+: error \[{rule}\][^\n]*a\.ts")).unwrap()
    };
    common::comply()
        .current_dir(root)
        .args(["--staged", "--diff-only"])
        .assert()
        .failure()
        .stdout(on_b_naming_a("no-duplicate-function"))
        .stdout(on_b_naming_a("no-clones"))
        .stdout(on_b_naming_a("no-duplicate-comments"));
}

#[test]
fn unchanged_duplicates_stay_out_of_a_staged_scan_issue_8611() {
    let dir = repo_with_committed_original_8611();
    let root = dir.path();
    fs::write(root.join("b.ts"), duplicated_module_8611()).unwrap();
    git(root, &["add", "b.ts"]);
    git(root, &["commit", "-qm", "copy"]);
    fs::write(root.join("c.ts"), "export const unrelated = 1;\n").unwrap();
    git(root, &["add", "c.ts"]);

    common::comply()
        .current_dir(root)
        .arg("--staged")
        .assert()
        .stdout(predicate::str::contains("no-duplicate-function").not())
        .stdout(predicate::str::contains("no-clones").not())
        .stdout(predicate::str::contains("no-duplicate-comments").not());
}

#[test]
fn two_staged_copies_are_reported_once_on_the_last_path_issue_8611() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("a.ts"), duplicated_module_8611()).unwrap();
    fs::write(root.join("b.ts"), duplicated_module_8611()).unwrap();
    git(root, &["add", "a.ts", "b.ts"]);

    let out = common::comply()
        .current_dir(root)
        .args(["--staged", "--diff-only", "--json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let findings: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let on = |rule: &str| -> Vec<String> {
        findings
            .iter()
            .filter(|f| f["ruleId"] == rule)
            .map(|f| f["path"].as_str().unwrap().to_string())
            .collect()
    };
    for rule in ["no-duplicate-function", "no-clones"] {
        let files = on(rule);
        assert_eq!(files.len(), 1, "{rule}: {stdout}");
        assert!(files[0].ends_with("b.ts"), "{rule}: {stdout}");
    }
}

/// A one-crate workspace holding `files` (path, content).
fn rust_crate(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    for (path, content) in files {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    dir
}

const LEN_ZERO_IN_TEST: &str =
    "#[test]\nfn empty() {\n    let v: Vec<u8> = Vec::new();\n    assert!(v.len() == 0);\n}\n";

#[test]
fn disallowed_method_from_comply_toml_is_reported_issue_8614() {
    let dir = rust_crate(&[
        (
            "comply.toml",
            "[clippy]\ndisallowed-methods = [{ path = \"std::env::var\", reason = \"read Config\" }]\n",
        ),
        (
            "src/lib.rs",
            "pub fn home() -> Option<String> {\n    std::env::var(\"HOME\").ok()\n}\n",
        ),
    ]);

    common::comply()
        .current_dir(dir.path())
        .arg(".")
        .assert()
        .stdout(predicate::str::contains("clippy::disallowed_methods"))
        .stdout(predicate::str::contains("read Config"));
}

#[test]
fn project_clippy_toml_stops_comply_before_clippy_issue_8614() {
    let dir = rust_crate(&[
        (
            "clippy.toml",
            "disallowed-methods = [{ path = \"std::env::var\", reason = \"read Config\" }]\n",
        ),
        (
            "src/lib.rs",
            "pub fn is_none(v: &[u8]) -> bool {\n    v.len() == 0\n}\n",
        ),
    ]);

    common::comply()
        .current_dir(dir.path())
        .arg(".")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("clippy.toml"))
        .stderr(predicate::str::contains("disallowed-methods"))
        .stderr(predicate::str::contains("crashed unexpectedly").not())
        .stdout(predicate::str::contains("clippy::").not());
}

#[test]
fn clippy_reports_test_module_and_tests_dir_once_issue_8614() {
    let lib = format!(
        "pub fn one() -> u8 {{\n    1\n}}\n\n#[cfg(test)]\nmod tests {{\n{LEN_ZERO_IN_TEST}}}\n"
    );
    let dir = rust_crate(&[("src/lib.rs", &lib), ("tests/it.rs", LEN_ZERO_IN_TEST)]);

    let output = common::comply()
        .current_dir(dir.path())
        .arg(".")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let findings: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("clippy::len_zero"))
        .collect();
    assert_eq!(
        findings.iter().filter(|l| l.contains("lib.rs")).count(),
        1,
        "{stdout}"
    );
    assert_eq!(
        findings.iter().filter(|l| l.contains("it.rs")).count(),
        1,
        "{stdout}"
    );
}

#[test]
fn fix_rewrites_clippy_findings_in_tests_dir_issue_8614() {
    let dir = rust_crate(&[
        ("src/lib.rs", "pub fn one() -> u8 {\n    1\n}\n"),
        ("tests/it.rs", LEN_ZERO_IN_TEST),
    ]);

    // `cargo clippy --fix` refuses a package outside any VCS.
    git(dir.path(), &["init", "-q"]);

    common::comply()
        .current_dir(dir.path())
        .args(["--fix", "."])
        .assert()
        .stderr(predicate::str::contains("clippy --fix failed").not());

    let fixed = fs::read_to_string(dir.path().join("tests/it.rs")).unwrap();
    assert!(fixed.contains("is_empty()"), "{fixed}");
}

#[test]
fn fix_reads_the_clippy_table_of_comply_toml_issue_8614() {
    // `manual_range_contains` needs Rust 1.35, so msrv 1.30 keeps the range.
    let dir = rust_crate(&[
        ("comply.toml", "[clippy]\nmsrv = \"1.30\"\n"),
        (
            "src/lib.rs",
            "pub fn small(x: u32) -> bool {\n    x >= 1 && x < 10\n}\n\n\
             pub fn none(v: &[u8]) -> bool {\n    v.len() == 0\n}\n",
        ),
    ]);
    git(dir.path(), &["init", "-q"]);

    common::comply()
        .current_dir(dir.path())
        .args(["--fix", "."])
        .assert()
        .stderr(predicate::str::contains("clippy --fix failed").not());

    let fixed = fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
    assert!(fixed.contains("is_empty()"), "{fixed}");
    assert!(fixed.contains("x >= 1 && x < 10"), "{fixed}");
}

/// A crate whose integration test needs the `extra` feature.
/// That test compares a length to zero.
fn feature_gated_crate() -> TempDir {
    let dir = rust_crate(&[
        (
            "Cargo.toml",
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [features]\nextra = []\n\n[workspace]\n",
        ),
        (
            "src/lib.rs",
            "#[cfg(feature = \"extra\")]\npub fn extra() -> Vec<u8> {\n    Vec::new()\n}\n",
        ),
        (
            "tests/it.rs",
            "#[test]\nfn empty() {\n    assert!(fixture::extra().len() == 0);\n}\n",
        ),
    ]);
    git(dir.path(), &["init", "-q"]);
    dir
}

#[test]
fn clippy_lints_a_test_that_requires_a_feature() {
    let dir = feature_gated_crate();

    common::comply()
        .current_dir(dir.path())
        .arg(".")
        .assert()
        .stdout(predicate::str::contains("clippy::len_zero"));
}

#[test]
fn fix_rewrites_a_test_that_requires_a_feature() {
    let dir = feature_gated_crate();

    common::comply()
        .current_dir(dir.path())
        .args(["--fix", "."])
        .assert()
        .stderr(predicate::str::contains("clippy --fix failed").not());

    let fixed = fs::read_to_string(dir.path().join("tests/it.rs")).unwrap();
    assert!(fixed.contains("is_empty()"), "{fixed}");
}
