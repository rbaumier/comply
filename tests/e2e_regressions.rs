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
    fs::write(dir.path().join("index.ts"), "export const ENTRY = 1;\n").unwrap();
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
    fs::create_dir_all(dir.path().join("node-pg-migrate/src/operations/tables")).unwrap();
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
            predicate::str::is_match(r"001_add_age\.ts:\d+:\d+:.*\[migration-needs-lock-timeout\]")
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

/// `line:column` of each `sql-no-truncate-in-app` finding.
/// Comply runs on `source`, alone at `file`.
fn truncate_positions(file: &str, source: &str) -> Vec<String> {
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
    let output = common::comply().arg(dir.path()).output().unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("[sql-no-truncate-in-app]"))
        .filter_map(|line| line.split(&format!("{file}:")).nth(1))
        .filter_map(|rest| rest.split(": ").next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn sql_truncate_as_comment_trigger_event_or_privilege_is_not_reported() {
    let migration = "-- A row trigger never sees TRUNCATE.\n\
                     /* TRUNCATE would empty the trail. */\n\
                     CREATE OR REPLACE TRIGGER audit_log_guard_truncate\n\
                     \x20   BEFORE TRUNCATE ON audit.audit_log\n\
                     \x20   FOR EACH STATEMENT EXECUTE FUNCTION audit.guard();\n\
                     CREATE TRIGGER audit_log_guard_delete\n\
                     \x20   AFTER DELETE OR TRUNCATE ON audit.audit_log\n\
                     \x20   FOR EACH STATEMENT EXECUTE FUNCTION audit.guard();\n\
                     REVOKE TRUNCATE ON audit.audit_log FROM netir_writer;\n\
                     GRANT SELECT, TRUNCATE ON audit.audit_log TO netir_owner;\n";

    let positions = truncate_positions("migrations/003.sql", migration);
    assert!(positions.is_empty(), "{positions:#?}");
}

#[test]
fn sql_truncate_statement_is_reported_on_its_own_line() {
    let migration = "-- Reset.\n\
                     TRUNCATE TABLE a;\n\
                     TRUNCATE b, c CASCADE;\n\
                     TRUNCATE ONLY d;\n\
                     SELECT '--'; TRUNCATE f;\n\
                     CREATE FUNCTION wipe() RETURNS void LANGUAGE plpgsql AS $$\n\
                     BEGIN\n\
                     \x20   -- TRUNCATE in a comment is no statement.\n\
                     \x20   TRUNCATE e;\n\
                     END;\n\
                     $$;\n";

    assert_eq!(
        truncate_positions("migrations/003.sql", migration),
        ["2:1", "3:1", "4:1", "5:14", "9:5"]
    );
}

#[test]
fn sql_truncate_that_postgres_or_mysql_runs_is_reported() {
    // Each TRUNCATE runs on the engine named.
    // One engine's comment can be the other's code.
    // So it must stay visible.
    let cases = [
        // PostgreSQL: `\'` escapes the quote of an `E'…'` string.
        (
            "SELECT E'it\\'s -- x'; TRUNCATE TABLE users;\n",
            vec!["1:23"],
        ),
        // PostgreSQL: the closing `$$` ends what opened inside the string.
        ("SELECT $$ /* $$;\nTRUNCATE TABLE users;\n", vec!["2:1"]),
        ("SELECT $$--$$; TRUNCATE TABLE users;\n", vec!["1:16"]),
        (
            "COMMENT ON TABLE users IS $$User's table$$;\n\
             UPDATE settings SET pat = '/*';\n\
             TRUNCATE TABLE sessions;\n\
             TRUNCATE TABLE tokens;\n",
            vec!["3:1", "4:1"],
        ),
        // MySQL runs the content of `/*! … */`.
        ("/*!40000 TRUNCATE TABLE users */;\n", vec!["1:10"]),
        // MySQL: the first `*/` closes the comment.
        ("/* a /* b */ TRUNCATE TABLE users; -- */\n", vec!["1:14"]),
        // MySQL: a backslash escapes the quote of a `'…'` string.
        (
            "SELECT 'it\\'s -- x'; TRUNCATE TABLE users;\n",
            vec!["1:22"],
        ),
        // MySQL: `--` opens a comment only before a blank.
        ("SELECT 5--1; TRUNCATE TABLE users;\n", vec!["1:14"]),
        // An unclosed `/*` is an error to both: it hides nothing.
        ("SELECT 1; /* forgot\nTRUNCATE TABLE users;\n", vec!["2:1"]),
    ];
    for (migration, expected) in cases {
        assert_eq!(
            truncate_positions("migrations/003.sql", migration),
            expected,
            "{migration}"
        );
    }
}

#[test]
fn sql_truncate_in_a_mariadb_versioned_comment_is_reported() {
    // MariaDB runs the content of `/*M! … */`, as MySQL runs `/*! … */`.
    let migration = "/*M!100100 TRUNCATE TABLE audit_log */;\n";
    assert_eq!(
        truncate_positions("migrations/010.sql", migration),
        ["1:12"]
    );
}

#[test]
fn sql_truncate_that_postgres_runs_with_backslash_escapes_is_reported() {
    // With `standard_conforming_strings` off, PostgreSQL reads `\'` as a quote.
    // Its string then closes after `--`.
    // `#` is no comment to it.
    let migration = "SELECT 'a\\' -- ' # 1; TRUNCATE TABLE audit_log;\n";
    assert_eq!(
        truncate_positions("migrations/011.sql", migration),
        ["1:23"]
    );
}

#[test]
fn sql_truncate_in_a_hyphenated_name_is_no_statement() {
    // A rule id like `sql-no-truncate-in-app` holds the word.
    // It runs nothing.
    let named = "SELECT 'sql-no-truncate-in-app';\n";
    assert!(truncate_positions("migrations/012.sql", named).is_empty());
    // To PostgreSQL, `--x` is a comment.
    // The statement goes on.
    let commented = "TRUNCATE--x\nTABLE audit_log;\n";
    assert_eq!(truncate_positions("migrations/013.sql", commented), ["1:1"]);
}

#[test]
fn sql_truncate_followed_by_on_or_a_comma_or_a_parenthesis_is_no_statement() {
    // `ON`, `OR` or `,` follow a privilege or trigger event.
    // `(` follows MySQL's function.
    // A statement is followed by `TABLE`, `ONLY` or a name.
    // The word before it does not matter.
    let migration = "DELETE FROM dbo.after\n\
                     TRUNCATE TABLE users\n\
                     DELETE FROM t; -- or\n\
                     TRUNCATE TABLE sessions;\n\
                     CREATE PROCEDURE p() BEGIN DECLARE EXIT HANDLER FOR SQLEXCEPTION TRUNCATE TABLE t; END;\n\
                     GRANT TRUNCATE, SELECT ON t TO u;\n\
                     REVOKE GRANT OPTION FOR TRUNCATE ON t FROM u;\n\
                     CREATE TRIGGER g BEFORE TRUNCATE OR DELETE ON t FOR EACH STATEMENT EXECUTE FUNCTION g();\n\
                     SELECT TRUNCATE(price, 2) FROM items;\n";

    assert_eq!(
        truncate_positions("migrations/003.sql", migration),
        ["2:1", "4:1", "5:66"]
    );
}

#[test]
fn sql_truncate_in_rust_and_ts_literals_is_read_after_their_escapes() {
    // Once the literal is decoded, `\n` is a newline.
    // It ends `--` comments and splits `TRUNCATE` from the prior word.
    let rust = "pub fn a() -> &'static str { \"BEGIN;\\nTRUNCATE TABLE users;\\nCOMMIT;\" }\n\
                pub fn b() -> &'static str { \"-- TRUNCATE TABLE users;\\nSELECT 1\" }\n";
    assert_eq!(truncate_positions("src/q.rs", rust), ["1:30"]);

    let ts = "export const a = `DELETE FROM sessions; -- reset\\n  TRUNCATE TABLE users;`;\n\
              export const b = `BEGIN;\\nTRUNCATE TABLE users;`;\n\
              export const c = `SELECT $$ /* $$; TRUNCATE TABLE users;`;\n\
              export const d = `-- TRUNCATE TABLE users;\\nSELECT ${1}`;\n\
              export const e = \"-- TRUNCATE TABLE users;\\nSELECT 1\";\n";
    assert_eq!(truncate_positions("src/q.ts", ts), ["1:18", "2:18", "3:18"]);
}

#[test]
fn sql_truncate_in_a_module_named_after_benchmarks_is_reported() {
    // Cargo builds benchmarks from the package's top `benches/` only.
    // A `src/` module named after benchmarks ships.
    let reset = "pub const RESET: &str = \"TRUNCATE TABLE cpu_runs;\";\n";
    assert_eq!(
        truncate_positions("src/reports/cpu_bench.rs", reset),
        ["1:25"]
    );
}

#[test]
fn sql_truncate_in_a_migration_named_after_tests_is_reported() {
    // SQL names no test file.
    // Only a test directory exempts it.
    let truncate = "TRUNCATE TABLE speed_samples;\n";
    assert_eq!(
        truncate_positions("migrations/V7__reset_speed_test.sql", truncate),
        ["1:1"]
    );
    assert!(truncate_positions("db/tests/reset.sql", truncate).is_empty());
}

#[test]
fn sql_truncate_in_a_module_a_binary_ships_is_reported() {
    // `lib.rs` builds `helper` for its tests only.
    // `main.rs` builds it into the release binary.
    let dir = rust_crate(&[
        ("src/lib.rs", "#[cfg(test)]\nmod helper;\n"),
        (
            "src/main.rs",
            "mod helper;\nfn main() {\n    println!(\"{}\", helper::RESET);\n}\n",
        ),
        (
            "src/helper.rs",
            "pub const RESET: &str = \"TRUNCATE TABLE sessions;\";\n",
        ),
    ]);
    let output = common::comply().arg(dir.path()).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let reported = stdout.lines().any(|line| {
        line.contains("src/helper.rs:1:25") && line.contains("[sql-no-truncate-in-app]")
    });
    assert!(reported, "{stdout}");
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
        .stdout(
            predicate::str::is_match(r"rete\.config\.ts:1:")
                .unwrap()
                .not(),
        )
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
    fs::write(dir.path().join("src/index.ts"), "export const one = 1;\n").unwrap();

    // Whether `node` itself or the package is what's absent decides which
    // sentence comply prints; both are toolchain gaps and both must exit 2
    // through the actionable path.
    common::comply()
        .arg(dir.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "comply: type-aware analysis needs",
        ))
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
fn project_clippy_toml_beside_clippy_table_stops_comply_before_clippy_issue_8614() {
    let dir = rust_crate(&[
        ("comply.toml", "[clippy]\nmsrv = \"1.70\"\n"),
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

/// Two methods: one calls `std::env::var`, one takes three arguments.
const ENV_VAR_AND_THREE_ARGS: &str = "pub fn home() -> Option<String> {\n    std::env::var(\"HOME\").ok()\n}\n\n\
     pub fn first(a: u8, _b: u8, _c: u8) -> u8 {\n    a\n}\n";

// Without comply.toml, clippy keeps the project's own config.
#[test]
fn project_clippy_toml_without_comply_toml_reaches_clippy() {
    let dir = rust_crate(&[
        (
            ".clippy.toml",
            "disallowed-methods = [{ path = \"std::env::var\", reason = \"read Config\" }]\n\
             too-many-arguments-threshold = 2\n",
        ),
        ("src/lib.rs", ENV_VAR_AND_THREE_ARGS),
    ]);

    common::comply()
        .current_dir(dir.path())
        .arg(".")
        .assert()
        .stderr(predicate::str::contains(".clippy.toml").not())
        .stdout(predicate::str::contains("clippy::disallowed_methods"))
        .stdout(predicate::str::contains("read Config"))
        .stdout(predicate::str::contains("too many arguments (3/2)"));
}

#[test]
fn threshold_also_set_by_project_clippy_toml_stops_comply_before_clippy() {
    let dir = rust_crate(&[
        (
            "comply.toml",
            "[rules.\"clippy::too_many_arguments\"]\nthreshold = 5\n",
        ),
        (".clippy.toml", "too-many-arguments-threshold = 2\n"),
        ("src/lib.rs", ENV_VAR_AND_THREE_ARGS),
    ]);

    common::comply()
        .current_dir(dir.path())
        .arg(".")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "`too-many-arguments-threshold` is set both by [rules.\"clippy::too_many_arguments\"]",
        ))
        .stderr(predicate::str::contains(".clippy.toml; keep one"))
        .stdout(predicate::str::contains("clippy::").not());
}

// Clippy rejects the key and lints nothing.
// The error names the project file, never comply's cached copy.
#[test]
fn unknown_key_in_project_clippy_toml_is_an_error_naming_that_file() {
    let dir = rust_crate(&[
        (
            ".clippy.toml",
            "msrv = \"1.70\"\ntoo-many-arguments-treshold = 2\n",
        ),
        ("src/lib.rs", ENV_VAR_AND_THREE_ARGS),
    ]);

    common::comply()
        .current_dir(dir.path())
        .arg(".")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(".clippy.toml:2:1: unknown field"))
        .stderr(predicate::str::contains("`too-many-arguments-threshold`"))
        .stderr(predicate::str::contains("clippy-conf").not())
        .stdout(predicate::str::contains("all clear").not())
        .stdout(predicate::str::contains("clippy::").not());
}

// Clippy reads `.clippy.toml` and ignores `clippy.toml` beside it.
#[test]
fn clippy_toml_beside_dot_clippy_toml_is_an_error_naming_both() {
    let dir = rust_crate(&[
        (".clippy.toml", "too-many-arguments-threshold = 2\n"),
        (
            "clippy.toml",
            "disallowed-methods = [{ path = \"std::env::var\", reason = \"read Config\" }]\n",
        ),
        ("src/lib.rs", ENV_VAR_AND_THREE_ARGS),
    ]);

    common::comply()
        .current_dir(dir.path())
        .arg(".")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(".clippy.toml and "))
        .stderr(predicate::str::contains(
            "clippy.toml both configure clippy",
        ))
        .stdout(predicate::str::contains("clippy::").not());
}

// `cargo shear` exits 2 when `cargo metadata` fails.
// It prints no report: no dependency was checked.
#[cfg(unix)]
#[test]
fn cargo_shear_that_checks_nothing_is_an_error() {
    use std::os::unix::fs::PermissionsExt;
    let dir = rust_crate(&[("src/lib.rs", "pub fn f() {}\n")]);
    let bin = TempDir::new().unwrap();
    let shear = bin.path().join("cargo-shear");
    fs::write(
        &shear,
        "#!/bin/sh\ncase \"$*\" in *--version*) echo 'cargo-shear 1.14.0'; exit 0;; esac\n\
         echo 'Error: Metadata error: `cargo metadata` exited with an error:'\nexit 2\n",
    )
    .unwrap();
    fs::set_permissions(&shear, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin.path().to_path_buf()];
    dirs.extend(std::env::split_paths(&path));

    common::comply()
        .current_dir(dir.path())
        .env("PATH", std::env::join_paths(dirs).unwrap())
        .arg(".")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cargo shear could not check"))
        .stderr(predicate::str::contains("It exited with 2"))
        .stderr(predicate::str::contains("Metadata error"));
}

// Plain clippy reads a member's own clippy.toml for that member.
// comply hands one file to the whole workspace.
// So that member file would go unread.
#[test]
fn member_clippy_toml_is_an_error_naming_it() {
    let dir = rust_crate(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"m\"]\nresolver = \"2\"\n",
        ),
        (
            "m/Cargo.toml",
            "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("m/clippy.toml", "too-many-arguments-threshold = 2\n"),
        ("m/src/lib.rs", ENV_VAR_AND_THREE_ARGS),
    ]);

    common::comply()
        .current_dir(dir.path())
        .arg(".")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("m/clippy.toml"))
        .stderr(predicate::str::contains("workspace member"))
        .stdout(predicate::str::contains("clippy::").not());
}

// Plain clippy seeks its file up to the filesystem root.
#[test]
fn clippy_toml_above_the_workspace_root_reaches_clippy() {
    let dir = rust_crate(&[
        (".clippy.toml", "too-many-arguments-threshold = 2\n"),
        (
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
        ),
        ("app/src/lib.rs", ENV_VAR_AND_THREE_ARGS),
    ]);

    common::comply()
        .current_dir(dir.path().join("app"))
        .arg(".")
        .assert()
        .stdout(predicate::str::contains("too many arguments (3/2)"));
}

// An empty `[clippy]` still adopts comply.toml as the single source.
#[test]
fn project_clippy_toml_beside_empty_clippy_table_is_an_error() {
    let dir = rust_crate(&[
        ("comply.toml", "[clippy]\n"),
        ("clippy.toml", "too-many-arguments-threshold = 2\n"),
        ("src/lib.rs", ENV_VAR_AND_THREE_ARGS),
    ]);

    common::comply()
        .current_dir(dir.path())
        .arg(".")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("clippy.toml is not read"))
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

#[test]
fn clippy_that_cannot_compile_is_an_error_not_all_clear() {
    // rustc's own errors carry no `clippy::` code.
    // comply dropped them and printed "all clear".
    // It must fail and say why.
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"broken\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/lib.rs"),
        "/// Returns one.\npub fn one() -> u32 {\n    \"one\"\n}\n",
    )
    .unwrap();

    common::comply()
        .arg(dir.path())
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .assert()
        .code(2)
        .stdout(predicate::str::contains("all clear").not())
        .stderr(predicate::str::contains("cargo clippy could not compile"))
        .stderr(predicate::str::contains("mismatched types"));
}

/// A crate whose `sqlx::query!` asks a database at build time.
/// `.env` names a dead one, but `.sqlx/` holds the query.
fn sqlx_crate() -> TempDir {
    rust_crate(&[
        (
            "Cargo.toml",
            "[package]\nname = \"sessions\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\nsqlx = { version = \"0.8\", default-features = false, \
             features = [\"macros\", \"postgres\", \"runtime-tokio\"] }\n\n[workspace]\n",
        ),
        (".env", "DATABASE_URL=postgres://127.0.0.1:1/sessions\n"),
        (
            "src/lib.rs",
            "//! Session store.\n\n\
             /// Deletes every session, and returns how many went.\n\
             pub async fn purge(pool: &sqlx::PgPool) -> Result<u64, sqlx::Error> {\n\
             \x20   let done = sqlx::query!(\"DELETE FROM sessions\").execute(pool).await?;\n\
             \x20   Ok(done.rows_affected())\n\
             }\n\n\
             /// True when `v` holds nothing.\n\
             pub fn is_none(v: &[u8]) -> bool {\n\
             \x20   v.len() == 0\n\
             }\n",
        ),
        // `sqlx` names the entry after the SHA-256 of the query.
        (
            ".sqlx/query-a6953b8d45e8ccf9da305fe0e9e2d7661063317a48cb96448d06da043f39edff.json",
            "{\n  \"db_name\": \"PostgreSQL\",\n  \"query\": \"DELETE FROM sessions\",\n  \
             \"describe\": {\n    \"columns\": [],\n    \"parameters\": {\n      \"Left\": []\n    },\n    \
             \"nullable\": []\n  },\n  \
             \"hash\": \"a6953b8d45e8ccf9da305fe0e9e2d7661063317a48cb96448d06da043f39edff\"\n}\n",
        ),
    ])
}

#[test]
fn clippy_builds_a_sqlx_crate_from_its_query_cache() {
    let dir = sqlx_crate();
    common::comply()
        .arg(dir.path())
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .env_remove("SQLX_OFFLINE")
        .env_remove("DATABASE_URL")
        .assert()
        .stderr(predicate::str::contains("could not compile").not())
        .stdout(predicate::str::contains("clippy::len_zero"));
}

#[test]
fn fix_builds_a_sqlx_crate_from_its_query_cache() {
    let dir = sqlx_crate();
    // `cargo clippy --fix` refuses a package outside any VCS.
    git(dir.path(), &["init", "-q"]);
    common::comply()
        .current_dir(dir.path())
        .args(["--fix", "."])
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .env_remove("SQLX_OFFLINE")
        .env_remove("DATABASE_URL")
        .assert()
        .stderr(predicate::str::contains("could not compile").not());

    let fixed = fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
    assert!(fixed.contains("v.is_empty()"), "{fixed}");
}
