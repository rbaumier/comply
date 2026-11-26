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

/// `line:column` of each `rule` finding of comply on `source`.
/// `source` is written at `file` in an empty project.
fn positions(rule: &str, file: &str, source: &str) -> Vec<String> {
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
    let output = common::comply().arg(dir.path()).output().unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(&format!("[{rule}]")))
        .filter_map(|line| line.split(&format!("{file}:")).nth(1))
        .filter_map(|rest| rest.split(": ").next())
        .map(str::to_owned)
        .collect()
}

fn truncate_positions(file: &str, source: &str) -> Vec<String> {
    positions("sql-no-truncate-in-app", file, source)
}

#[test]
fn sql_comply_ignore_markers_are_sql_comments() {
    let migration = "-- comply-ignore-file: sql-create-index-concurrently — the table is empty at deploy\n\
                     CREATE INDEX idx_a ON t(a);\n\
                     -- comply-ignore: sql-no-truncate-in-app — resets the fixture table\n\
                     TRUNCATE TABLE fixtures;\n\
                     TRUNCATE TABLE sessions; -- comply-ignore: sql-no-truncate-in-app\n";
    let file = "migrations/005.sql";

    assert!(positions("sql-create-index-concurrently", file, migration).is_empty());
    assert!(truncate_positions(file, migration).is_empty());
    assert_eq!(
        positions("comply-ignore-missing-justification", file, migration),
        ["5:26"]
    );
}

#[test]
fn dockerfile_comply_ignore_marker_is_a_hash_comment() {
    let dockerfile = "FROM alpine:3.20\n\
                      # comply-ignore: dockerfile-absolute-workdir — the base image sets an absolute WORKDIR\n\
                      WORKDIR app\n\
                      WORKDIR src\n";

    let lines: Vec<String> = positions("dockerfile-absolute-workdir", "Dockerfile", dockerfile)
        .iter()
        .filter_map(|position| position.split(':').next())
        .map(str::to_owned)
        .collect();

    assert_eq!(lines, ["4"]);
}

#[test]
fn a_comply_ignore_marker_counts_only_in_a_comment_of_its_language() {
    // Each case comments the marker, then writes it as text.
    // In text, it changes nothing the rule reports.
    let compose = "services:\n  web:\n    image: nginx:1.25.3\n";
    let package = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n";
    let ip = "const host = \"10.1.2.3\";\n";
    let vue = |script: String| {
        format!(
            "<script setup lang=\"ts\">\n{script}</script>\n<template><p>{{{{ host }}}}</p></template>\n"
        )
    };
    let cases = [
        (
            "ops/compose.yml",
            "compose-no-privileged",
            format!("{compose}    # @\n    privileged: true\n"),
            format!("{compose}    command: |\n      # @\n      run\n    privileged: true\n"),
        ),
        (
            "ops/compose.yml",
            "compose-no-privileged",
            format!("{compose}    # @\n    privileged: true\n"),
            format!("{compose}    labels:\n      a# @\n    privileged: true\n"),
        ),
        (
            "Cargo.toml",
            "cargo-release-profile-lto",
            format!("# @\n{package}"),
            format!("{package}description = \"\"\"\n# @\n\"\"\"\n"),
        ),
        (
            "src/lib.rs",
            "no-timing-attack",
            format!("// @\n{PLAIN_COMPARE}"),
            format!("pub const DOC: &str = r#\"\n// @\n\"#;\n{PLAIN_COMPARE}"),
        ),
        (
            "web/net.ts",
            "no-hardcoded-ip",
            format!("// @\n{ip}"),
            format!("const doc = `\n// @\n`;\n{ip}"),
        ),
        (
            "web/Net.vue",
            "no-hardcoded-ip",
            vue(format!("// @\n{ip}")),
            vue(format!("const doc = `\n// @\n`;\n{ip}")),
        ),
        (
            "app.css",
            "css-no-empty-block",
            "/* @ */\n.b {}\n".to_owned(),
            ".a { content: \"x\\\n/* @ */\"; }\n.b {}\n".to_owned(),
        ),
        (
            "Dockerfile",
            "dockerfile-apk-no-cache",
            "FROM alpine:3.20\n# @\nRUN apk add curl\n".to_owned(),
            "FROM alpine:3.20\nCOPY <<EOF /etc/motd\n# @\nEOF\nRUN apk add curl\n".to_owned(),
        ),
        (
            "Dockerfile",
            "dockerfile-apk-no-cache",
            "FROM alpine:3.20\n# @\nRUN apk add curl\n".to_owned(),
            "FROM alpine:3.20\nRUN <<EOF\n# @\ntrue\nEOF\nRUN apk add curl\n".to_owned(),
        ),
        (
            "nginx/site.conf",
            "nginx-add-header-always",
            "server {\n  # @\n  add_header X-Frame-Options DENY;\n}\n".to_owned(),
            "server {\n  add_header X-Note \"a\n# @\";\n  add_header X-Frame-Options DENY;\n}\n"
                .to_owned(),
        ),
    ];
    for (file, rule, comment, text) in cases {
        assert_only_a_comment_silences(rule, file, &comment, &text);
    }
}

/// A marker in `comment` silences `rule`.
/// One in `text` does not.
/// `@` stands for the marker.
fn assert_only_a_comment_silences(rule: &str, file: &str, comment: &str, text: &str) {
    let marker = format!("comply-ignore-file: {rule} — kept on purpose");
    let (comment, text) = (comment.replace('@', &marker), text.replace('@', &marker));
    assert_eq!(
        positions(rule, file, &comment),
        Vec::<String>::new(),
        "{file}:\n{comment}"
    );
    let reported = positions(rule, file, &text);
    assert!(!reported.is_empty(), "{file}:\n{text}");
    let plain = text.replace("comply-ignore", "comply-notice");
    assert_eq!(reported, positions(rule, file, &plain), "{file}:\n{text}");
}

#[test]
fn a_dockerfile_marker_after_an_instruction_is_text() {
    for instruction in [
        "ARG A=b # @",
        "ENV NOTE x # @",
        "ENV NOTE=x # @",
        "EXPOSE 80 # @",
        "FROM alpine:3.20 # @",
        "LABEL a=b \\\n  note=x # @",
        "LABEL note=x # @",
        "USER app # @",
        "WORKDIR /app # @",
    ] {
        assert_only_a_comment_silences(
            "dockerfile-apk-no-cache",
            "Dockerfile",
            "FROM alpine:3.20\nRUN echo a && \\\n  # @\n  echo b\nRUN apk add curl\n",
            &format!("FROM alpine:3.20\n{instruction}\nRUN apk add curl\n"),
        );
    }
}

#[test]
fn a_css_marker_in_a_bare_url_is_text() {
    for url in ["url(\n/* @ */\n)", "URL(data:image/svg+xml,/* @ */)"] {
        assert_only_a_comment_silences(
            "css-no-empty-block",
            "app.css",
            ".a { background: url(\"a.png\"); } /* @ */\n.b {}\n",
            &format!(".a {{ background: {url}; }}\n.b {{}}\n"),
        );
    }
}

#[test]
fn an_nginx_marker_in_a_word_or_a_lua_block_is_text() {
    let header = "server {\n  add_header X-Frame-Options DENY;\n}\n";
    let lua = "content_by_lua_block {\n  local s = [[ }\n  # @\n  ]] -- }\n}\n";
    for text in ["return 301 ${host}# @\n;", "rewrite ^/(a{2}# @\n) /x;", lua] {
        assert_only_a_comment_silences(
            "nginx-add-header-always",
            "nginx/site.conf",
            &format!("server {{ content_by_lua_block {{ a = {{}} }} }} # @\n{header}"),
            &format!("server {{ {text} }}\n{header}"),
        );
    }
}

#[test]
fn a_vue_marker_outside_a_comment_is_text() {
    let image = "<img src=\"x.png\">";
    for text in [
        "<p title=\"\n<!-- @ -->\n\">a</p>",
        "<p>{{ `\n<!-- @ -->\n` }}</p>",
        "<textarea>\n<!-- @ -->\n</textarea>",
        "<p>\n// @\n</p>",
    ] {
        assert_only_a_comment_silences(
            "a11y-alt-text",
            "web/Net.vue",
            &format!("<template>\n<p>a</p> <!-- @ -->\n{image}\n</template>\n"),
            &format!("<template>\n{text}\n{image}\n</template>\n"),
        );
    }
    for block in [
        "<style>\n.a { content: \"\\\n<!-- @ -->\"; }\n</style>",
        "<style>\n.a { content: \"\\\n// @\"; }\n</style>",
        "<docs>\n// @\n<!-- @ -->\n</docs>",
    ] {
        assert_only_a_comment_silences(
            "a11y-alt-text",
            "web/Net.vue",
            &format!("<template>{image}</template>\n<style>\n/* // @ */\n</style>\n"),
            &format!("<template>{image}</template>\n{block}\n"),
        );
    }
}

#[test]
fn a_sql_marker_in_copy_data_or_another_language_is_text() {
    let truncate = "TRUNCATE TABLE sessions;\n";
    for text in [
        "COPY notes (body) FROM stdin;\n-- @\n\\.\n",
        "CREATE FUNCTION f() RETURNS text LANGUAGE plperl AS $$\nmy $s = q{\n-- @\n};\n$$;\n",
        "CREATE FUNCTION f() RETURNS text AS $$\nmy $s = q{\n-- @\n};\n$$ LANGUAGE plperl;\n",
    ] {
        assert_only_a_comment_silences(
            "sql-no-truncate-in-app",
            "migrations/012.sql",
            &format!("COPY notes (body) FROM stdin;\nit's\n\\.\n-- @\n{truncate}"),
            &format!("{text}{truncate}"),
        );
    }
}

#[test]
fn a_javascript_marker_in_jsx_text_is_text() {
    let ip = "export const host = \"10.1.2.3\";\n";
    assert_only_a_comment_silences(
        "no-hardcoded-ip",
        "web/net.js",
        &format!("export const Note = () => <p>it's</p>;\n// @\n{ip}"),
        &format!("export const Note = () => <p>\n// @\n</p>;\n{ip}"),
    );
}

#[test]
fn an_eslint_config_counts_only_in_a_comment() {
    let (rule, file) = ("no-hardcoded-ip", "web/net.ts");
    let ip = "export const host = \"10.1.2.3\";\n";
    let config = format!("/* eslint {rule}: 0 */");
    let comment = format!("{config}\n{ip}");
    let text = format!("export const doc = `\n{config}\n`;\n{ip}");
    assert!(positions(rule, file, &comment).is_empty());
    assert_eq!(positions(rule, file, &text), ["4:22"]);
}

#[test]
fn sql_rules_read_statements_not_comments() {
    // A statement in a comment is no statement.
    // A commented word does not change the statement beside it.
    // The rules that ask for a comment still read it.
    let migration = "-- CREATE INDEX idx_old ON users(email);\n\
                     /* DROP TABLE legacy;\n\
                     \x20  ALTER TABLE users DROP COLUMN nickname; */\n\
                     -- Lookups by email from the sign-in form.\n\
                     CREATE INDEX idx_email ON users(email); -- CONCURRENTLY next time\n\
                     CREATE TABLE notes (\n\
                     \x20 id BIGINT PRIMARY KEY,\n\
                     \x20 /* archived_at TIMESTAMP, */\n\
                     \x20 title TEXT NOT NULL\n\
                     );\n";
    let rules = [
        "sql-create-index-concurrently",
        "sql-drop-table-no-cascade-warning",
        "sql-no-drop-column-without-expand",
        "sql-index-needs-rationale-comment",
        "sql-nullable-requires-comment",
    ];

    let found: Vec<(&str, Vec<String>)> = rules
        .into_iter()
        .map(|rule| (rule, positions(rule, "migrations/004.sql", migration)))
        .collect();

    let expected: Vec<(&str, Vec<String>)> = rules
        .into_iter()
        .map(|rule| match rule {
            "sql-create-index-concurrently" => (rule, vec!["5:1".to_owned()]),
            _ => (rule, Vec::new()),
        })
        .collect();
    assert_eq!(found, expected);
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
fn sql_comply_ignore_marker_in_a_dollar_string_is_text() {
    // To PostgreSQL, `$$ … $$` is a string.
    // The marker in it is text.
    let truncate = "SELECT $$ -- comply-ignore: sql-no-truncate-in-app — not a comment $$; \
                    TRUNCATE TABLE audit_log;\n";
    assert_eq!(truncate_positions("migrations/006.sql", truncate), ["1:72"]);

    let drop = "SELECT $$ -- comply-ignore: sql-no-drop-column-without-expand — z $$; \
                ALTER TABLE accounts DROP COLUMN legacy_flag;\n";
    assert_eq!(
        positions(
            "sql-no-drop-column-without-expand",
            "migrations/007.sql",
            drop
        ),
        ["1:1"]
    );
}

#[test]
fn sql_finding_after_a_comment_with_a_multibyte_character_keeps_its_column() {
    // The mask blanks `é` to two spaces.
    // The column is counted in the file.
    let migration = "/* purge é */ TRUNCATE TABLE sessions;\n";
    assert_eq!(
        truncate_positions("migrations/014.sql", migration),
        ["1:15"]
    );
}

#[test]
fn sql_comply_ignore_marker_in_a_string_over_lines_is_text() {
    // The string opened on line 1 closes on line 3.
    // The rule reports a line, at column 1.
    let migration = "INSERT INTO notes(body) VALUES ('a\n\
                     -- comply-ignore: sql-create-index-concurrently — z\n\
                     '); CREATE INDEX idx_email ON users(email);\n";
    assert_eq!(
        positions(
            "sql-create-index-concurrently",
            "migrations/008.sql",
            migration
        ),
        ["3:1"]
    );
}

#[test]
fn sql_truncate_after_a_marker_in_a_string_is_reported_where_it_runs() {
    // The rule id in the string names `truncate`.
    // It runs nothing.
    let migration = "INSERT INTO notes(body) VALUES ('a\n\
                     -- comply-ignore: sql-no-truncate-in-app — z\n\
                     '); TRUNCATE TABLE audit_log;\n";
    assert_eq!(truncate_positions("migrations/009.sql", migration), ["3:5"]);
}

#[test]
fn sql_comply_ignore_marker_counts_in_a_routine_body_and_a_block_comment() {
    // A `DO` block or a function body runs as plpgsql.
    // There, `--` opens a comment.
    // A lone block comment aims at the next line.
    let honored = [
        "DO $$ BEGIN\n  -- comply-ignore: sql-no-truncate-in-app — rebuilt\n  TRUNCATE users;\nEND $$;\n",
        "CREATE FUNCTION wipe() RETURNS void LANGUAGE plpgsql AS $$\nBEGIN\n  \
         -- comply-ignore: sql-no-truncate-in-app — rebuilt\n  TRUNCATE users;\nEND;\n$$;\n",
        "/* -- comply-ignore: sql-no-truncate-in-app — rebuilt */\nTRUNCATE TABLE users;\n",
    ];
    for migration in honored {
        assert!(
            truncate_positions("migrations/010.sql", migration).is_empty(),
            "{migration}"
        );
    }
    // `//` opens no SQL comment, so its marker is text.
    let slashes = "// comply-ignore: sql-no-truncate-in-app — rebuilt\nTRUNCATE TABLE users;\n";
    assert_eq!(truncate_positions("migrations/011.sql", slashes), ["2:1"]);
}

#[test]
fn sql_boolean_column_prefix_is_reported_on_the_line_declaring_the_column() {
    // The `;` in the comment ends no statement.
    // comply-ignore: sql-boolean-column-prefix — the defect under test.
    let migration = "-- Holds wait for diarization; see the worker.\n\
                     CREATE TABLE holds (\n\
                     \x20 id BIGINT PRIMARY KEY,\n\
                     \x20 active BOOLEAN NOT NULL\n\
                     );\n";
    assert_eq!(
        positions("sql-boolean-column-prefix", "migrations/012.sql", migration),
        ["4:1"]
    );
}

/// The three rules that read SQL in Rust string literals.
const SQL_RUST_RULES: [&str; 3] = [
    "sql-no-drop-column-without-expand",
    "sql-index-needs-rationale-comment",
    "sql-no-truncate-in-app",
];

/// Sorted `file:line:column` of each `rule` finding in `files`.
fn located(files: &[(&str, &str)], rule: &str) -> Vec<String> {
    let mut found: Vec<String> = findings(files, rule)
        .iter()
        .filter_map(|line| line.split(": ").next())
        .map(str::to_owned)
        .collect();
    found.sort();
    found
}

#[test]
fn sql_rules_report_rust_code_that_ships_whatever_its_name() {
    // A migration or a module named after tests ships.
    // `cfg_attr(test, …)` applies an attribute: the item ships too.
    let migration = "migration/src/m20250101_000001_drop_test_mode.rs";
    let files = [
        (
            migration,
            "pub const UP: &str = \"ALTER TABLE accounts DROP COLUMN test_mode;\";\n\
             pub const IDX: &str = \"CREATE INDEX idx_accounts_email ON accounts(email);\";\n",
        ),
        (
            "src/ab_test/reset.rs",
            "pub fn reset() -> &'static str {\n    \"TRUNCATE TABLE experiments;\"\n}\n",
        ),
        (
            "src/db.rs",
            "#[cfg_attr(test, allow(dead_code))]\n\
             pub fn purge_sessions() -> &'static str {\n    \"TRUNCATE TABLE sessions;\"\n}\n",
        ),
    ];

    let found = SQL_RUST_RULES.map(|rule| located(&files, rule));
    assert_eq!(
        found,
        [
            vec![format!("{migration}:1:22")],
            vec![format!("{migration}:2:24")],
            vec!["src/ab_test/reset.rs:2:5".into(), "src/db.rs:3:5".into()],
        ]
    );
}

#[test]
fn sql_rules_report_a_rust_module_named_like_a_test_file() {
    // Cargo builds `src/speed_test.rs` as a module.
    // It is no test target.
    let files = [(
        "src/speed_test.rs",
        "pub const RESET: &str = \"TRUNCATE TABLE samples;\";\n\
         pub const UP: &str = \"ALTER TABLE samples DROP COLUMN legacy;\";\n\
         pub const IDX: &str = \"CREATE INDEX idx_samples_at ON samples(at);\";\n",
    )];
    let found = SQL_RUST_RULES.map(|rule| located(&files, rule));
    assert_eq!(
        found,
        [
            ["src/speed_test.rs:2:22"],
            ["src/speed_test.rs:3:24"],
            ["src/speed_test.rs:1:25"],
        ]
    );
}

/// The `sql-no-truncate-in-app` findings with `gate` on the item.
/// Then with `gate` on the `mod` declaration of its file.
fn truncate_findings_under(gate: &str) -> [Vec<String>; 2] {
    let body = "pub fn reset() -> &'static str {\n    \"TRUNCATE TABLE sessions;\"\n}\n";
    let gated_item = format!("{gate}\n{body}");
    let gated_module = format!("{gate}\nmod db;\n");
    [
        located(&[("src/db.rs", &gated_item)], "sql-no-truncate-in-app"),
        located(
            &[("src/lib.rs", &gated_module), ("src/db.rs", body)],
            "sql-no-truncate-in-app",
        ),
    ]
}

#[test]
fn sql_rules_exempt_rust_code_under_cfg_test() {
    let none: Vec<String> = Vec::new();
    assert_eq!(
        truncate_findings_under("#[cfg(test)]"),
        [none.clone(), none]
    );
}

#[test]
fn sql_rules_exempt_rust_code_under_cfg_all_test() {
    // `all` needs each option, so `test` too.
    let none: Vec<String> = Vec::new();
    assert_eq!(
        truncate_findings_under("#[cfg(all(test, unix))]"),
        [none.clone(), none]
    );
}

#[test]
fn sql_rules_report_rust_code_under_cfg_any_test() {
    // `any` builds the item with the feature alone, outside tests.
    assert_eq!(
        truncate_findings_under("#[cfg(any(test, feature = \"seed\"))]"),
        [["src/db.rs:3:5"], ["src/db.rs:2:5"]]
    );
}

#[test]
fn sql_rules_report_rust_code_under_cfg_not_test() {
    assert_eq!(
        truncate_findings_under("#[cfg(not(test))]"),
        [["src/db.rs:3:5"], ["src/db.rs:2:5"]]
    );
}

/// The sorted files with a `rule` finding among `files`.
fn files_reported(files: &[(&str, &str)], rule: &str) -> Vec<String> {
    let mut found: Vec<String> = located(files, rule)
        .iter()
        .filter_map(|place| place.split(':').next())
        .map(str::to_owned)
        .collect();
    found.dedup();
    found
}

const CONCAT_SQL: &str = "pub fn load(id: u64) -> String {\n    format!(\"SELECT name FROM users WHERE id = {}\", id)\n}\n";
const PLAIN_COMPARE: &str =
    "pub fn check(password: &str, input: &str) -> bool {\n    password == input\n}\n";

#[test]
fn rust_security_rules_report_shipped_code_named_after_tests() {
    // A module named after tests or docs ships.
    // `cfg_attr(test, …)` applies an attribute: the item ships too.
    // Only the `#[cfg(test)] mod speed_test;` gate keeps code out.
    let under_cfg_attr = |body: &str| format!("#[cfg_attr(test, allow(dead_code))]\n{body}");
    let db = under_cfg_attr(CONCAT_SQL);
    let auth = under_cfg_attr(PLAIN_COMPARE);
    let witness = format!("{CONCAT_SQL}{PLAIN_COMPARE}");
    let files = [
        (
            "src/lib.rs",
            "pub mod ab_test;\npub mod auth;\npub mod db;\npub mod docs;\n#[cfg(test)]\nmod speed_test;\n",
        ),
        ("src/ab_test/mod.rs", "pub mod query;\n"),
        ("src/ab_test/query.rs", CONCAT_SQL),
        ("src/db.rs", db.as_str()),
        ("src/auth.rs", auth.as_str()),
        ("src/docs/mod.rs", PLAIN_COMPARE),
        ("src/speed_test.rs", witness.as_str()),
    ];
    assert_eq!(
        files_reported(&files, "db-no-string-concat-sql"),
        ["src/ab_test/query.rs", "src/db.rs"]
    );
    assert_eq!(
        files_reported(&files, "no-timing-attack"),
        ["src/auth.rs", "src/docs/mod.rs"]
    );
}

#[test]
fn security_and_data_rules_judge_rust_and_sql_names_by_the_build() {
    // The other security rules skipped these paths by name alone.
    // The `#[cfg(test)] mod fixtures;` gate still keeps its module out.
    let weak_hash = "pub fn digest(data: &[u8]) -> [u8; 16] {\n    md5::compute(data).0\n}\n";
    let logging = "pub fn login(password: &str) {\n    log::info!(\"login {}\", password);\n}\n";
    let debug_secret = "#[derive(Debug)]\npub struct ApiKey {\n    pub value: String,\n}\n";
    let shipped = format!("{weak_hash}{logging}{debug_secret}");
    let secret = "pub const API_KEY: &str = \"abcd1234567890abcdef\";\n";
    let varchar = "CREATE TABLE speed_samples (name VARCHAR(20));\n";
    let fixtures = format!("{shipped}{secret}");
    let files = [
        (
            "src/lib.rs",
            "pub mod ab_test;\npub mod docs;\npub mod keys;\n#[cfg(test)]\nmod fixtures;\n",
        ),
        ("src/ab_test/mod.rs", "pub mod hash_test;\n"),
        ("src/ab_test/hash_test.rs", shipped.as_str()),
        ("src/docs/mod.rs", shipped.as_str()),
        ("src/keys/mod.rs", "pub mod demo;\n"),
        ("src/keys/demo/mod.rs", secret),
        ("src/fixtures/mod.rs", fixtures.as_str()),
        ("migrations/V7__speed_test.sql", varchar),
        ("db/tests/seed.sql", varchar),
    ];
    for rule in [
        "no-weak-hashing",
        "no-confidential-logging",
        "rust-secret-type-derives-debug",
    ] {
        assert_eq!(
            files_reported(&files, rule),
            ["src/ab_test/hash_test.rs", "src/docs/mod.rs"],
            "{rule}"
        );
    }
    assert_eq!(
        files_reported(&files, "no-hardcoded-secret"),
        ["src/keys/demo/mod.rs"]
    );
    assert_eq!(
        files_reported(&files, "sql-no-varchar"),
        ["migrations/V7__speed_test.sql"]
    );
}

#[test]
fn rust_runtime_rules_report_shipped_modules_named_after_tests_or_docs() {
    // These rules also skipped Rust paths by name alone.
    // The `#[cfg(test)] mod fixtures;` gate still keeps its module out.
    let body = "pub fn matcher(pattern: &str) { let _r = Regex::new(pattern); }\n\
                pub fn read(p: *const u8) -> u8 { unsafe { *p } }\n\
                pub fn url() -> String { std::env::var(\"URL\").unwrap() }\n\
                pub async fn fetch(url: &str) { let _r = reqwest::get(url).await; }\n\
                pub fn git() { std::process::Command::new(\"git\").output().ok(); }\n\
                pub fn queue() { let _q = tokio::sync::mpsc::unbounded_channel::<u8>(); }\n";
    let files = [
        (
            "src/lib.rs",
            "pub mod api_test;\npub mod docs;\npub mod examples;\n#[cfg(test)]\nmod fixtures;\n",
        ),
        ("src/api_test.rs", body),
        ("src/docs/mod.rs", body),
        ("src/examples/mod.rs", body),
        ("src/fixtures/mod.rs", body),
    ];
    let shipped = ["src/api_test.rs", "src/docs/mod.rs", "src/examples/mod.rs"];
    for rule in [
        "no-new-regex-with-variable",
        "rust-undocumented-unsafe",
        "rust-env-var-unwrap-at-runtime",
        "timeout-on-io",
        "timeout-on-external-command",
        "rust-unbounded-channel",
    ] {
        assert_eq!(files_reported(&files, rule), shipped, "{rule}");
    }
}

const APP_MANIFEST: &str =
    "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n";
const UNSAFE_READ: &str = "pub fn first(v: &[u8]) -> u8 {\n    unsafe { *v.get_unchecked(0) }\n}\n";

#[test]
fn a_fuzz_engine_the_release_build_leaves_out_keeps_the_crate_judged() {
    // A fuzz harness has `cargo-fuzz = true` or a required engine.
    // A dev or optional engine leaves `src/` shipping.
    let engine_manifest = "[package]\nname = \"libfuzzer-sys\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [lib]\npath = \"lib.rs\"\n";
    let command = "pub fn list() -> std::io::Result<std::process::Output> {\n    \
                   std::process::Command::new(\"ls\").output()\n}\n";
    let reported = |deps: &str, rule: &str| {
        let manifest = format!("{APP_MANIFEST}\n{deps}");
        files_reported(
            &[
                ("Cargo.toml", manifest.as_str()),
                ("vendor/libfuzzer-sys/Cargo.toml", engine_manifest),
                ("vendor/libfuzzer-sys/lib.rs", ""),
                ("src/lib.rs", "pub mod auth;\npub mod cmd;\npub mod raw;\n"),
                ("src/auth.rs", PLAIN_COMPARE),
                ("src/cmd.rs", command),
                ("src/raw.rs", UNSAFE_READ),
            ],
            rule,
        )
    };
    let dev = "[dev-dependencies]\nlibfuzzer-sys = { path = \"vendor/libfuzzer-sys\" }\n";
    let optional =
        "[dependencies]\nlibfuzzer-sys = { path = \"vendor/libfuzzer-sys\", optional = true }\n";
    for deps in [dev, optional] {
        assert_eq!(
            reported(deps, "no-timing-attack"),
            ["src/auth.rs"],
            "{deps}"
        );
        assert_eq!(
            reported(deps, "rust-undocumented-unsafe"),
            ["src/raw.rs"],
            "{deps}"
        );
        assert_eq!(
            reported(deps, "timeout-on-external-command"),
            ["src/cmd.rs"],
            "{deps}"
        );
    }
    let required = "[dependencies]\nlibfuzzer-sys = { path = \"vendor/libfuzzer-sys\" }\n";
    assert!(reported(required, "no-timing-attack").is_empty());
}

#[test]
fn a_binary_declared_under_examples_ships_with_its_modules() {
    // `[[bin]] path =` builds a release target in any directory.
    // `examples/demo.rs` stays an example, out of release.
    let manifest =
        format!("{APP_MANIFEST}\n[[bin]]\nname = \"server\"\npath = \"examples/server.rs\"\n");
    let defects = format!("{PLAIN_COMPARE}{UNSAFE_READ}pub static mut COUNTER: u32 = 0;\n");
    let server = format!("mod auth;\n{defects}fn main() {{}}\n");
    let demo = format!("{defects}fn main() {{}}\n");
    let files = [
        ("Cargo.toml", manifest.as_str()),
        ("src/lib.rs", ""),
        ("examples/server.rs", server.as_str()),
        ("examples/auth/mod.rs", defects.as_str()),
        ("examples/demo.rs", demo.as_str()),
    ];
    for rule in [
        "no-timing-attack",
        "rust-undocumented-unsafe",
        "rust-no-static-mut",
    ] {
        assert_eq!(
            files_reported(&files, rule),
            ["examples/auth/mod.rs", "examples/server.rs"],
            "{rule}"
        );
    }
}

#[test]
fn rust_rules_judge_a_docs_module_the_library_ships() {
    // A `src/` module named `docs` ships with the library.
    // Cargo builds `examples/` and `benches/` out of release.
    // A top `docs/` file belongs to no target.
    let unsafe_code = "pub struct Handle(pub *mut u8);\nunsafe impl Send for Handle {}\n\
                       pub static mut COUNTER: u32 = 0;\n";
    let target = format!("{unsafe_code}fn main() {{}}\n");
    let files = [
        ("Cargo.toml", APP_MANIFEST),
        ("src/lib.rs", "pub mod docs;\npub mod sync;\n"),
        ("src/docs/mod.rs", unsafe_code),
        ("src/sync.rs", unsafe_code),
        ("examples/demo.rs", target.as_str()),
        ("benches/load.rs", target.as_str()),
        ("docs/snippet.rs", unsafe_code),
    ];
    for rule in ["rust-unsafe-impl-without-comment", "rust-no-static-mut"] {
        assert_eq!(
            files_reported(&files, rule),
            ["src/docs/mod.rs", "src/sync.rs"],
            "{rule}"
        );
    }
}

const LIST_FILES: &str = "pub fn list() -> std::io::Result<std::process::Output> {\n    \
                          std::process::Command::new(\"ls\").output()\n}\n";
const SELECT_ALL: &str = "pub fn by_id(id: &str) -> String {\n    format!(\"SELECT * FROM users WHERE id = {}\", id)\n}\n";
const RESET_SESSIONS: &str = "pub const RESET: &str = \"TRUNCATE TABLE sessions;\";\n";

/// Rules that judge only the code a build ships.
const SHIP_RULES: [&str; 6] = [
    "db-no-string-concat-sql",
    "no-timing-attack",
    "rust-undocumented-unsafe",
    "sql-no-select-star",
    "sql-no-truncate-in-app",
    "timeout-on-external-command",
];

/// One defect for each rule of [`SHIP_RULES`].
fn shipped_defects() -> String {
    format!("{PLAIN_COMPARE}{UNSAFE_READ}{LIST_FILES}{SELECT_ALL}{RESET_SESSIONS}")
}

/// The `rules` reporting `file` in a project of `files`.
fn rules_reporting(files: &[(&str, &str)], file: &str, rules: &[&str]) -> Vec<String> {
    let lines = report(files);
    rules
        .iter()
        .filter(|rule| {
            lines.iter().any(|line| {
                line.starts_with(&format!("{file}:")) && line.contains(&format!("[{rule}]"))
            })
        })
        .map(|rule| (*rule).to_owned())
        .collect()
}

#[test]
fn a_module_one_release_root_declares_ships_whatever_gate_another_puts_on_it() {
    // A `#[path]` declaration names its file.
    // A `[lib] path` root declares modules too.
    // One ungated declaration ships the file.
    let helper = shipped_defects();
    let tool = "#[path = \"../helper.rs\"]\nmod helper;\n\
                fn main() {\n    println!(\"{}\", helper::check(\"a\", \"b\"));\n}\n";
    let core_manifest = format!("{APP_MANIFEST}\n[lib]\npath = \"src/core.rs\"\n");
    let gated = "#[cfg(test)]\nmod helper;\n";
    let renamed = "#[cfg(test)]\nmod helper;\n#[path = \"helper.rs\"]\npub mod runtime;\n";
    let projects = [
        vec![
            ("Cargo.toml", APP_MANIFEST),
            ("src/lib.rs", gated),
            ("src/bin/tool.rs", tool),
        ],
        vec![("Cargo.toml", APP_MANIFEST), ("src/lib.rs", renamed)],
        vec![
            ("Cargo.toml", core_manifest.as_str()),
            ("src/core.rs", "pub mod helper;\n"),
            ("src/main.rs", "#[cfg(test)]\nmod helper;\nfn main() {}\n"),
        ],
    ];
    for mut files in projects {
        files.push(("src/helper.rs", helper.as_str()));
        assert_eq!(
            rules_reporting(&files, "src/helper.rs", &SHIP_RULES),
            SHIP_RULES,
            "{files:?}"
        );
    }
}

#[test]
fn a_file_outside_src_a_release_root_declares_ships() {
    // `#[path]` can reach into `tests/` or `examples/`.
    // An inline `mod` block nests the files it declares.
    let helper = shipped_defects();
    let no_examples = APP_MANIFEST.replace("edition", "autoexamples = false\nedition");
    let server_manifest =
        format!("{APP_MANIFEST}\n[[bin]]\nname = \"server\"\npath = \"examples/server.rs\"\n");
    let server = "mod api {\n    pub mod auth;\n}\n\
                  fn main() {\n    let _ = api::auth::check(\"a\", \"b\");\n}\n";
    let projects = [
        (
            "tests/common/mod.rs",
            vec![
                ("Cargo.toml", APP_MANIFEST),
                (
                    "src/lib.rs",
                    "#[path = \"../tests/common/mod.rs\"]\npub mod common;\n",
                ),
            ],
        ),
        (
            "examples/common.rs",
            vec![
                ("Cargo.toml", no_examples.as_str()),
                (
                    "src/lib.rs",
                    "#[path = \"../examples/common.rs\"]\npub mod common;\n",
                ),
            ],
        ),
        (
            "examples/api/auth.rs",
            vec![
                ("Cargo.toml", server_manifest.as_str()),
                ("src/lib.rs", ""),
                ("examples/server.rs", server),
            ],
        ),
    ];
    for (shipped, mut files) in projects {
        files.push((shipped, helper.as_str()));
        assert_eq!(
            rules_reporting(&files, shipped, &SHIP_RULES),
            SHIP_RULES,
            "{shipped}"
        );
    }
}

#[test]
fn a_file_a_macro_or_a_conditional_path_loads_ships() {
    // Another root gates the same file on `test`.
    // The library's own declaration ships it.
    let helper = shipped_defects();
    let macro_rules = "macro_rules! modules { ($($item:tt)*) => { $($item)* } }\n";
    let invoked = format!("{macro_rules}modules! {{ pub mod helper; }}\n");
    // The shape of the `cfg-if` crate, without the dependency.
    let cfg_if = "macro_rules! cfg_if {\n    \
                  (if #[cfg($meta:meta)] { $($item:item)* }) => { $(#[cfg($meta)] $item)* };\n}\n\
                  cfg_if! {\n    if #[cfg(unix)] {\n        pub mod helper;\n    }\n}\n";
    let reloaded = "#[path = \"a.rs\"]\npub mod renamed;\npub mod a;\n";
    let twin = "mod helper;";
    let projects = [
        (
            "src/helper.rs",
            twin,
            "#[cfg_attr(not(test), path = \"helper.rs\")]\npub mod db;\n",
        ),
        ("src/helper.rs", twin, cfg_if),
        ("src/helper.rs", twin, invoked.as_str()),
        ("src/helper.rs", twin, "include!(\"helper.rs\");\n"),
        (
            "src/api/helper.rs",
            "#[path = \"api/helper.rs\"]\nmod twin;",
            "#[path = \"api\"]\npub mod inner {\n    pub mod helper;\n}\n",
        ),
        ("src/a/b.rs", "#[path = \"a/b.rs\"]\nmod twin;", reloaded),
    ];
    for (shipped, twin, library) in projects {
        let main = format!("#[cfg(test)]\n{twin}\nfn main() {{}}\n");
        let files = [
            ("Cargo.toml", APP_MANIFEST),
            ("src/lib.rs", library),
            ("src/main.rs", main.as_str()),
            ("src/db.rs", ""),
            ("src/a.rs", "pub mod b;\n"),
            ("src/b.rs", ""),
            (shipped, helper.as_str()),
        ];
        assert_eq!(
            rules_reporting(&files, shipped, &SHIP_RULES),
            SHIP_RULES,
            "{library}"
        );
    }
}

#[test]
fn a_file_only_a_test_build_loads_does_not_ship() {
    // `#[path]` keeps the gate of its item.
    // So does an inline `mod`.
    // `[[test]] path` builds a test target in any directory.
    let helper = shipped_defects();
    let test_target = format!("{APP_MANIFEST}\n[[test]]\nname = \"api\"\npath = \"it/api.rs\"\n");
    let renamed = "#[cfg(test)]\n#[path = \"parse_tests.rs\"]\nmod tests;\n";
    let renamed_first = "#[path = \"parse_tests.rs\"]\n#[cfg(test)]\nmod tests;\n";
    let projects = [
        ("src/parse_tests.rs", APP_MANIFEST, renamed),
        ("src/parse_tests.rs", APP_MANIFEST, renamed_first),
        (
            "src/parse/unit.rs",
            APP_MANIFEST,
            "#[cfg(test)]\n#[path = \"parse/unit.rs\"]\nmod tests;\n",
        ),
        (
            "src/parse/checks/helper.rs",
            APP_MANIFEST,
            "#[cfg(test)]\nmod checks {\n    mod helper;\n}\n",
        ),
        ("it/api.rs", test_target.as_str(), ""),
    ];
    for (file, manifest, parse) in projects {
        let files = [
            ("Cargo.toml", manifest),
            ("src/lib.rs", "pub mod parse;\n"),
            ("src/parse.rs", parse),
            (file, helper.as_str()),
        ];
        let none: [&str; 0] = [];
        assert_eq!(
            rules_reporting(&files, file, &SHIP_RULES),
            none,
            "{file}: {parse}"
        );
    }
}

#[test]
fn a_module_named_like_a_test_file_ships_unless_a_test_gate_declares_it() {
    // `src/test_*.rs` is a module, not a test target.
    // Only its `mod` declaration decides.
    let queue = "pub fn queue() -> usize {\n    let (tx, rx) = std::sync::mpsc::channel();\n    \
                 for i in 0..3u8 {\n        let _ = tx.send(i);\n    }\n    \
                 drop(tx);\n    rx.try_iter().count()\n}\n";
    let body = format!("{}{queue}", shipped_defects());
    let main = "mod test_dir_leftover;\n#[cfg(test)]\nmod test_gated;\n\
                fn main() {\n    let _ = test_dir_leftover::queue();\n}\n";
    let files = [
        ("Cargo.toml", APP_MANIFEST),
        ("src/main.rs", main),
        ("src/test_dir_leftover.rs", body.as_str()),
        ("src/test_gated.rs", body.as_str()),
    ];
    let mut rules = SHIP_RULES.to_vec();
    rules.push("rust-unbounded-channel");
    assert_eq!(
        rules_reporting(&files, "src/test_dir_leftover.rs", &rules),
        rules
    );
    assert!(rules_reporting(&files, "src/test_gated.rs", &rules).is_empty());
}

#[test]
fn rust_unbounded_channel_reports_each_unbounded_constructor_and_no_bounded_one() {
    // An `as` rename names the same constructor.
    let body = |statement: &str| {
        format!(
            "pub fn run() {{\n    {statement}\n    for i in 0..3 {{\n        \
             let _ = tx.send(i);\n    }}\n    drop(rx);\n}}\n"
        )
    };
    let unbounded = [
        "let (tx, rx) = std::sync::mpsc::channel::<u8>();",
        "let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<u8>();",
        "let (tx, rx): (tokio::sync::mpsc::UnboundedSender<u8>, _) = \
         tokio::sync::mpsc::unbounded_channel();",
        "let (tx, rx) = crossbeam::channel::unbounded::<u8>();",
        "let (tx, rx): (crossbeam::channel::Sender<u8>, _) = crossbeam::channel::unbounded();",
        "let (tx, rx): (crossbeam_channel::Sender<u8>, _) = crossbeam_channel::unbounded();",
        "use tokio::sync::mpsc::unbounded_channel as uc;\n    let (tx, rx) = uc::<u8>();",
        "use crossbeam_channel::unbounded as ub;\n    let (tx, rx) = ub::<u8>();",
        "use std::sync::mpsc::channel as chan;\n    let (tx, rx) = chan::<u8>();",
        "use tokio::sync::mpsc as queue;\n    let (tx, rx) = queue::unbounded_channel::<u8>();",
    ];
    let bounded = [
        "let (tx, rx) = std::sync::mpsc::sync_channel::<u8>(4);",
        "let (tx, rx) = tokio::sync::mpsc::channel::<u8>(4);",
        "let (tx, rx) = crossbeam::channel::bounded::<u8>(4);",
        "use tokio::sync::mpsc::channel as uc;\n    let (tx, rx) = uc::<u8>(4);",
    ];
    let reported = |statement: &&str| {
        !positions("rust-unbounded-channel", "src/queue.rs", &body(statement)).is_empty()
    };
    let missed: Vec<_> = unbounded.iter().filter(|s| !reported(s)).collect();
    let flagged: Vec<_> = bounded.iter().filter(|s| reported(s)).collect();
    assert_eq!((missed, flagged), (vec![], vec![]));
}

// A rename binds its own scope only.
#[test]
fn rust_unbounded_channel_follows_no_rename_of_another_scope() {
    const IMPORT: &str = "use std::sync::mpsc;\n";
    const CALL: &str = "pub fn run() {\n    let (tx, rx) = mpsc::channel();\n    \
        for i in 0..3 {\n        let _ = tx.send(i);\n    }\n    drop(rx);\n}\n";
    let call_of = |name: &str| CALL.replace("mpsc::channel", name);
    let unbounded = [
        // Another module, a child module, then another function.
        format!("mod a {{\n    use crate::fake as mpsc;\n}}\n{IMPORT}{CALL}"),
        format!("{IMPORT}{CALL}mod a {{\n    use crate::fake as mpsc;\n}}\n"),
        format!("use crate::fake as mpsc;\nmod a {{\n{IMPORT}{CALL}}}\n"),
        format!("{IMPORT}fn other() {{\n    use crate::fake as mpsc;\n}}\n{CALL}"),
        // The nearer import wins.
        format!(
            "use crate::fake as mpsc;\n{}",
            CALL.replace("let (tx", "use std::sync::mpsc;\n    let (tx")
        ),
        // One build sees the rename, the other does not.
        format!("#[cfg(test)]\nuse crate::fake as mpsc;\n#[cfg(not(test))]\n{IMPORT}{CALL}"),
        format!(
            "#[cfg(test)]\nuse crate::fake as q;\n#[cfg(not(test))]\n\
             use std::sync::mpsc as q;\n{}",
            call_of("q::channel")
        ),
        format!(
            "mod a {{\n    use crate::fake as unbounded;\n}}\n\
             use flume::unbounded;\n{}",
            call_of("unbounded")
        ),
        // A rename seen from a block and a nested function.
        format!("use std::sync::mpsc as q;\n{}", call_of("q::channel")),
        CALL.replace(
            "let (tx, rx) = mpsc::channel();",
            "use std::sync::mpsc as q;\n    fn make() -> u8 {\n        \
             let (tx, rx) = q::channel();\n        for i in 0..3 {\n            \
             let _ = tx.send(i);\n        }\n        drop(rx);\n        0\n    }\n    \
             let (tx, rx) = (make(), 0);",
        ),
    ];
    let bounded = [
        format!("use crate::fake as mpsc;\n{CALL}"),
        format!(
            "{IMPORT}{}",
            CALL.replace("let (tx", "use crate::fake as mpsc;\n    let (tx")
        ),
    ];
    let reported =
        |source: &&String| !positions("rust-unbounded-channel", "src/queue.rs", source).is_empty();
    let missed: Vec<_> = unbounded.iter().filter(|s| !reported(s)).collect();
    let flagged: Vec<_> = bounded.iter().filter(|s| reported(s)).collect();
    assert_eq!((missed, flagged), (vec![], vec![]));
}

/// What comply reports for `rule` in a project of `files`.
/// Each line leaves out the project root.
fn findings(files: &[(&str, &str)], rule: &str) -> Vec<String> {
    report(files)
        .into_iter()
        .filter(|line| line.contains(&format!("[{rule}]")))
        .collect()
}

/// What comply reports in a project of `files`.
/// Each line leaves out the project root.
fn report(files: &[(&str, &str)]) -> Vec<String> {
    let dir = TempDir::new().expect("failed to create temp dir");
    for (name, content) in files {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let output = common::comply().arg(dir.path()).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let roots = [
        fs::canonicalize(dir.path()).unwrap().display().to_string(),
        dir.path().display().to_string(),
    ];
    stdout
        .lines()
        .filter_map(|line| {
            roots
                .iter()
                .find_map(|root| line.strip_prefix(root.as_str()))
        })
        .map(|rest| rest.trim_start_matches('/').to_owned())
        .collect()
}

/// The files reported `unused-file` in a project of `files`.
/// They are relative to its root, and sorted.
fn unused_files(files: &[(&str, &str)]) -> Vec<String> {
    let mut found: Vec<String> = findings(files, "unused-file")
        .iter()
        .filter_map(|line| line.split(':').next())
        .map(str::to_owned)
        .collect();
    found.sort();
    found
}

/// `<file> <export>` for each `dead-export` finding, sorted.
/// The project is made of `files`.
fn dead_exports(files: &[(&str, &str)]) -> Vec<String> {
    let mut found: Vec<String> = findings(files, "dead-export")
        .iter()
        .filter_map(|line| {
            Some(format!(
                "{} {}",
                line.split(':').next()?,
                line.split('`').nth(1)?
            ))
        })
        .collect();
    found.sort();
    found
}

/// The manifest of a React Router 7 app in framework mode.
const REACT_ROUTER_PACKAGE: &str = r#"{"name":"web","private":true,"type":"module","dependencies":{"react-router":"^7.9.3"},"devDependencies":{"@react-router/dev":"^7.9.5"}}"#;

#[test]
fn react_router_conventions_and_routes_ts_modules_are_entry_points() {
    let found = unused_files(&[
        (
            "front/package.json",
            r#"{"name":"front","private":true,"type":"module","dependencies":{"react-router":"^7.9.3"},"devDependencies":{"@react-router/dev":"^7.9.5"}}"#,
        ),
        (
            "front/react-router.config.ts",
            "export default { ssr: false };\n",
        ),
        (
            "front/app/root.tsx",
            "import { Shell } from \"./components/shell\";\nexport default function App() { return Shell; }\n",
        ),
        (
            "front/app/components/shell.tsx",
            "export const Shell = null;\n",
        ),
        (
            "front/app/entry.client.tsx",
            "import { boot } from \"./lib/boot\";\nboot();\n",
        ),
        (
            "front/app/lib/boot.ts",
            "export function boot() { return 1; }\n",
        ),
        (
            "front/app/routes.ts",
            "import { type RouteConfig, index, layout, route } from \"@react-router/dev/routes\";\n\
             // route(\"/old\", \"routes/orphan.tsx\"),\n\
             export default [\n\
             \x20 route(\"/home\", \"routes/home.tsx\"),\n\
             \x20 layout(\"layouts/frame.tsx\", [index(\"routes/index.tsx\")]),\n\
             ] satisfies RouteConfig;\n",
        ),
        (
            "front/app/routes/home.tsx",
            "import { format } from \"../lib/format\";\nexport default function Home() { return format(\"home\"); }\n",
        ),
        (
            "front/app/lib/format.ts",
            "export function format(s: string) { return s; }\n",
        ),
        (
            "front/app/routes/index.tsx",
            "export default function Index() { return null; }\n",
        ),
        (
            "front/app/layouts/frame.tsx",
            "export default function Frame() { return null; }\n",
        ),
        (
            "front/app/routes/orphan.tsx",
            "export default function Orphan() { return null; }\n",
        ),
        ("front/app/lib/dead.ts", "export const dead = 1;\n"),
    ]);
    assert_eq!(
        found,
        ["front/app/lib/dead.ts", "front/app/routes/orphan.tsx"]
    );
}

#[test]
fn react_router_app_directory_is_read_from_its_config() {
    let configs = [
        "import type { Config } from \"@react-router/dev/config\";\n\
         export default {\n\
         \x20 // appDirectory: \"app\",\n\
         \x20 appDirectory: \"./src\",\n\
         } satisfies Config;\n",
        "export default { \"appDirectory\": \"./src\" };\n",
    ];
    for config in configs {
        let found = unused_files(&[
            (
                "package.json",
                r#"{"name":"web","private":true,"type":"module","dependencies":{"react-router":"^7.9.3"}}"#,
            ),
            ("react-router.config.ts", config),
            (
                "src/root.tsx",
                "export default function App() { return null; }\n",
            ),
            (
                "src/entry.server.tsx",
                "export default function handleRequest() { return null; }\n",
            ),
            (
                "src/routes.ts",
                "import { route } from \"@react-router/dev/routes\";\nexport default [route(\"/\", \"./pages/home.tsx\")];\n",
            ),
            (
                "src/pages/home.tsx",
                "export default function Home() { return null; }\n",
            ),
            ("src/orphan.tsx", "export const orphan = 1;\n"),
            (
                "app/root.tsx",
                "export default function Stale() { return null; }\n",
            ),
        ]);
        assert_eq!(found, ["app/root.tsx", "src/orphan.tsx"], "{config}");
    }
}

#[test]
fn react_router_string_outside_a_route_file_argument_names_no_route_module() {
    // Only the file argument of `route()`, `index()` and `layout()` counts.
    // A path pattern, option or constant names no module.
    let found = unused_files(&[
        ("package.json", REACT_ROUTER_PACKAGE),
        (
            "app/root.tsx",
            "export default function App() { return null; }\n",
        ),
        (
            "app/routes.ts",
            "import { index, layout, route as page } from \"@react-router/dev/routes\";\n\
             const ARCHIVED = [\"routes/archived.tsx\"];\n\
             export default [\n\
             \x20 page(\"routes/pattern.tsx\", \"routes/live.tsx\", { id: \"routes/id.tsx\" }),\n\
             \x20 layout(\"layouts/frame.tsx\", [index(\"routes/home.tsx\")]),\n\
             ];\n",
        ),
        (
            "app/routes/live.tsx",
            "export default function Live() { return null; }\n",
        ),
        (
            "app/routes/home.tsx",
            "export default function Home() { return null; }\n",
        ),
        (
            "app/layouts/frame.tsx",
            "export default function Frame() { return null; }\n",
        ),
        (
            "app/routes/archived.tsx",
            "export default function Archived() { return null; }\n",
        ),
        (
            "app/routes/pattern.tsx",
            "export default function Pattern() { return null; }\n",
        ),
        (
            "app/routes/id.tsx",
            "export default function Id() { return null; }\n",
        ),
    ]);
    assert_eq!(
        found,
        [
            "app/routes/archived.tsx",
            "app/routes/id.tsx",
            "app/routes/pattern.tsx"
        ]
    );
}

#[test]
fn react_router_routes_split_across_imported_files_are_entry_points() {
    // Paths in a config `routes.ts` imports are app-relative too.
    let found = unused_files(&[
        ("package.json", REACT_ROUTER_PACKAGE),
        (
            "app/root.tsx",
            "export default function App() { return null; }\n",
        ),
        (
            "app/routes.ts",
            "import { type RouteConfig, prefix, route } from \"@react-router/dev/routes\";\n\
             import { adminRoutes } from \"./config/admin\";\n\
             export default [\n\
             \x20 route(\"/\", \"routes/home.tsx\"),\n\
             \x20 ...prefix(\"/admin\", adminRoutes),\n\
             ] satisfies RouteConfig;\n",
        ),
        (
            "app/config/admin.ts",
            "import { route } from \"@react-router/dev/routes\";\n\
             import { billing } from \"./billing\";\n\
             export const adminRoutes = [route(\"users\", \"routes/admin/users.tsx\"), ...billing];\n",
        ),
        (
            "app/config/billing.ts",
            "import { index } from \"@react-router/dev/routes\";\n\
             export const billing = [index(\"routes/admin/billing.tsx\")];\n",
        ),
        (
            "app/routes/home.tsx",
            "export default function Home() { return null; }\n",
        ),
        (
            "app/routes/admin/users.tsx",
            "export default function Users() { return null; }\n",
        ),
        (
            "app/routes/admin/billing.tsx",
            "export default function Billing() { return null; }\n",
        ),
        (
            "app/routes/admin/orphan.tsx",
            "export default function Orphan() { return null; }\n",
        ),
    ]);
    assert_eq!(found, ["app/routes/admin/orphan.tsx"]);
}

#[test]
fn react_router_flat_routes_are_the_entries_of_its_routes_directory() {
    // `flatRoutes()` routes each file directly in its directory.
    // Each folder there adds its `route` module, else its `index`.
    // Other folder files, deeper files and ignored files are not.
    let page = "export default function Page() { return null; }\n";
    let found = unused_files(&[
        ("package.json", REACT_ROUTER_PACKAGE),
        ("app/root.tsx", page),
        (
            "app/routes.ts",
            "import { type RouteConfig } from \"@react-router/dev/routes\";\n\
             import { flatRoutes } from \"@react-router/fs-routes\";\n\
             export default flatRoutes() satisfies RouteConfig;\n",
        ),
        ("app/routes/_index.tsx", page),
        ("app/routes/blog.$slug.tsx", page),
        (
            "app/routes/app.projects/route.tsx",
            "import { Card } from \"./project-card\";\nexport default function Projects() { return Card; }\n",
        ),
        (
            "app/routes/app.projects/project-card.tsx",
            "export const Card = null;\n",
        ),
        ("app/routes/app.projects/stale.tsx", page),
        ("app/routes/docs/index.tsx", page),
        ("app/routes/docs/nested/page.tsx", page),
    ]);
    assert_eq!(
        found,
        [
            "app/routes/app.projects/stale.tsx",
            "app/routes/docs/nested/page.tsx"
        ]
    );

    let found = unused_files(&[
        ("package.json", REACT_ROUTER_PACKAGE),
        ("app/root.tsx", page),
        (
            "app/routes.ts",
            "import { type RouteConfig, route } from \"@react-router/dev/routes\";\n\
             import { flatRoutes } from \"@react-router/fs-routes\";\n\
             export default [\n\
             \x20 route(\"/hello\", \"./routes/hello.tsx\"),\n\
             \x20 ...(await flatRoutes({ rootDirectory: \"fs-routes\", ignoredRouteFiles: [\"**/*.draft.tsx\"] })),\n\
             ] satisfies RouteConfig;\n",
        ),
        ("app/routes/hello.tsx", page),
        ("app/routes/other.tsx", page),
        ("app/fs-routes/about.tsx", page),
        ("app/fs-routes/about.draft.tsx", page),
    ]);
    assert_eq!(
        found,
        ["app/fs-routes/about.draft.tsx", "app/routes/other.tsx"]
    );
}

#[test]
fn react_router_flat_routes_with_a_glob_beyond_globset_names_no_route() {
    // React Router reads `ignoredRouteFiles` with minimatch.
    // globset does not read an extglob.
    // So the routes are unknown.
    // The files stay ordinary files.
    let page = "export default function Page() { return null; }\n";
    let found = unused_files(&[
        ("package.json", REACT_ROUTER_PACKAGE),
        ("app/root.tsx", page),
        (
            "app/routes.ts",
            "import { type RouteConfig } from \"@react-router/dev/routes\";\n\
             import { flatRoutes } from \"@react-router/fs-routes\";\n\
             export default flatRoutes({ ignoredRouteFiles: [\"**/*.@(draft|wip).tsx\"] }) satisfies RouteConfig;\n",
        ),
        ("app/routes/about.tsx", page),
        ("app/routes/about.draft.tsx", page),
    ]);
    assert_eq!(
        found,
        ["app/routes/about.draft.tsx", "app/routes/about.tsx"]
    );
}

#[test]
fn react_router_flat_routes_with_options_beyond_literals_names_no_route() {
    // A variable may skip files or move the directory.
    // So the routes are unknown, as for an extglob.
    let page = "export default function Page() { return null; }\n";
    let calls = [
        "const IGNORED = [\"**/*.draft.tsx\"];\nexport default flatRoutes({ ignoredRouteFiles: IGNORED });\n",
        "const DRAFT = \"**/*.draft.tsx\";\nexport default flatRoutes({ ignoredRouteFiles: [DRAFT] });\n",
        "const OPTIONS = { ignoredRouteFiles: [\"**/*.draft.tsx\"] };\nexport default flatRoutes(OPTIONS);\n",
        "const OPTIONS = { ignoredRouteFiles: [\"**/*.draft.tsx\"] };\nexport default flatRoutes({ ...OPTIONS });\n",
    ];
    for call in calls {
        let routes = format!("import {{ flatRoutes }} from \"@react-router/fs-routes\";\n{call}");
        let found = unused_files(&[
            ("package.json", REACT_ROUTER_PACKAGE),
            ("app/root.tsx", page),
            ("app/routes.ts", &routes),
            ("app/routes/about.tsx", page),
            ("app/routes/about.draft.tsx", page),
        ]);
        assert_eq!(
            found,
            ["app/routes/about.draft.tsx", "app/routes/about.tsx"],
            "{call}"
        );
    }
}

#[test]
fn react_router_route_module_exports_are_read_by_name() {
    // React Router reads these route module exports by name.
    // `Layout` is read in the root route only.
    // `routes.ts` and `entry.server` have their own names.
    // Elsewhere, such a name is ordinary.
    let found = dead_exports(&[
        ("package.json", REACT_ROUTER_PACKAGE),
        (
            "app/root.tsx",
            "export const links = () => [];\n\
             export function Layout() { return null; }\n\
             export function ErrorBoundary() { return null; }\n\
             export default function App() { return null; }\n",
        ),
        (
            "app/entry.server.tsx",
            "export const streamTimeout = 5000;\n\
             export function handleError() { return null; }\n\
             export default function handleRequest() { return null; }\n\
             export const extra = 1;\n",
        ),
        (
            "app/routes.ts",
            "import { route } from \"@react-router/dev/routes\";\n\
             export default [route(\"/demo\", \"routes/demo.tsx\")];\n",
        ),
        (
            "app/routes/demo.tsx",
            "import { used } from \"../lib/meta\";\n\
             export async function clientLoader() { return used; }\n\
             export const meta = () => [];\n\
             export const handle = { crumb: \"demo\" };\n\
             export default function Demo() { return null; }\n\
             export function Layout() { return null; }\n\
             export const helper = 1;\n",
        ),
        (
            "app/lib/meta.ts",
            "export const used = 1;\nexport const meta = 2;\n",
        ),
    ]);
    assert_eq!(
        found,
        [
            "app/entry.server.tsx extra",
            "app/lib/meta.ts meta",
            "app/routes/demo.tsx Layout",
            "app/routes/demo.tsx helper"
        ]
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
fn a_linted_file_that_is_not_utf8_is_refused_by_name() {
    for file in [
        "q.sql",
        "a.css",
        "c.yml",
        "Dockerfile",
        "nginx/site.conf",
        "Net.vue",
    ] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"# caf\xe9\n").unwrap();
        common::comply()
            .current_dir(dir.path())
            .arg(".")
            .assert()
            .code(2)
            .stdout(predicate::str::contains("all clear").not())
            .stderr(predicate::str::contains(file))
            .stderr(predicate::str::contains("valid UTF-8"))
            .stderr(predicate::str::contains("crashed unexpectedly").not());
    }
}

#[cfg(unix)]
#[test]
fn a_linted_file_without_read_permission_is_refused_by_name() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("q.sql");
    fs::write(&file, "SELECT id FROM users;\n").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
    common::comply()
        .current_dir(dir.path())
        .arg(".")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("q.sql"));
}

#[test]
fn an_unread_path_comply_lints_nothing_of_stops_no_run() {
    // A submodule can be named like a script.
    // A staged file can be gone from the working tree.
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("vendor")).unwrap();
    fs::write(root.join("vendor/q.sql"), b"-- caf\xe9\n").unwrap();
    fs::write(root.join("gone.sql"), "SELECT id FROM users;\n").unwrap();
    git(root, &["add", "-A"]);
    fs::remove_file(root.join("gone.sql")).unwrap();
    fs::create_dir_all(root.join("three.css")).unwrap();
    let submodule = format!("160000,{},three.css", "1".repeat(40));
    git(root, &["update-index", "--add", "--cacheinfo", &submodule]);
    common::comply()
        .current_dir(root)
        .arg("--staged")
        .assert()
        .success();
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
    // NODE_PATH could resolve typescript-go: keep the gap genuine.
    common::comply()
        .env_remove("NODE_PATH")
        .arg(dir.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "comply: type-aware analysis needs",
        ))
        .stderr(predicate::str::contains("crashed unexpectedly").not())
        .stderr(predicate::str::contains("github.com/rbaumier/comply/issues").not());
}

/// A parameter typed `any`, sniffed by `typeof` at 2:10.
const SNIFFS_ANY: &str = "export function size(value: any): number {\n  \
    return typeof value === \"string\" ? value.length : 0;\n}\n";

/// Run `ts-no-typeof-operator` over `tree`, written to a temp dir.
/// It needs typescript-go, or comply refuses the run.
fn typeof_findings(tree: &[(&str, &str)]) -> (Option<i32>, String, String) {
    let dir = TempDir::new().unwrap();
    for (rel, text) in tree {
        let file = dir.path().join(rel);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    let out = common::comply()
        .current_dir(dir.path())
        .args(["rules", "ts-no-typeof-operator", "."])
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// A file a `references` entry holds is checked there.
// That project declares `BUILD`, so line 4 is typed.
#[test]
fn type_aware_checks_a_file_a_reference_holds() {
    let config = format!("{SNIFFS_ANY}export const built = typeof BUILD === \"object\";\n");
    let (code, out, err) = typeof_findings(&[
        (
            "tsconfig.json",
            "{\"files\": [], \"references\": [{\"path\": \"./tsconfig.app.json\"}, \
             {\"path\": \"./tsconfig.node.json\"}]}\n",
        ),
        (
            "tsconfig.app.json",
            "{\"compilerOptions\": {\"composite\": true, \"strict\": true}, \
             \"include\": [\"src\"]}\n",
        ),
        (
            "tsconfig.node.json",
            "{\"compilerOptions\": {\"composite\": true, \"strict\": true}, \
             \"include\": [\"vite.config.ts\", \"build.d.ts\"]}\n",
        ),
        ("build.d.ts", "declare const BUILD: { id: string };\n"),
        ("src/main.ts", SNIFFS_ANY),
        ("vite.config.ts", &config),
    ]);
    assert_eq!(code, Some(1), "{out}{err}");
    for kept in ["src/main.ts:2:10:", "vite.config.ts:2:10:"] {
        assert!(out.contains(kept), "{kept} missing:\n{out}{err}");
    }
    assert!(!out.contains("vite.config.ts:4:"), "{out}");
}

// A file outside every `include` takes its nearest tsconfig's options.
// Its `paths` alias then resolves, so line 5 is typed.
#[test]
fn type_aware_checks_a_file_outside_include_with_the_nearest_options() {
    let tool = format!(
        "{SNIFFS_ANY}import {{ helper }} from \"@lib/helper\";\n\
         export const kind = typeof helper === \"function\";\n"
    );
    let (code, out, err) = typeof_findings(&[
        (
            "tsconfig.json",
            "{\"compilerOptions\": {\"strict\": true, \"paths\": {\"@lib/*\": [\"./src/*\"]}}, \
             \"include\": [\"src\"]}\n",
        ),
        (
            "src/helper.ts",
            "export function helper(): number {\n  return 1;\n}\n",
        ),
        ("scripts/tool.ts", &tool),
    ]);
    assert_eq!(code, Some(1), "{out}{err}");
    assert!(out.contains("scripts/tool.ts:2:10:"), "{out}{err}");
    assert!(!out.contains("scripts/tool.ts:5:"), "{out}");
}

// JavaScript its tsconfig does not allow stays unchecked.
// Its project would not compile it either.
#[test]
fn type_aware_leaves_out_javascript_its_tsconfig_does_not_allow() {
    let (code, out, err) = typeof_findings(&[
        (
            "tsconfig.json",
            "{\"compilerOptions\": {\"strict\": true}, \"include\": [\"src\"]}\n",
        ),
        ("src/a.ts", SNIFFS_ANY),
        (
            "scripts/tool.js",
            "export const kind = typeof JSON.parse(\"1\") === \"string\";\n",
        ),
    ]);
    assert_eq!(code, Some(1), "{out}{err}");
    assert!(out.contains("src/a.ts:2:10:"), "{out}{err}");
    assert!(!out.contains("tool.js"), "{out}");
}

// A TypeScript file no program holds fails the run.
// The refusal names it, and only it.
#[test]
fn type_aware_refuses_a_file_no_program_holds() {
    let (code, out, err) = typeof_findings(&[
        (
            "a/tsconfig.json",
            "{\"compilerOptions\": {\"strict\": true}}\n",
        ),
        ("a/x.ts", SNIFFS_ANY),
        ("y.ts", SNIFFS_ANY),
    ]);
    assert_eq!(code, Some(2), "{out}{err}");
    assert!(err.contains("y.ts"), "{err}");
    assert!(!err.contains("x.ts"), "{err}");
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
    // `no-useless-rename` carries a safe fix, which `oxlint --fix` applies.
    fs::write(
        root.join("a.ts"),
        "export function f(o: { a: number }) { const { a: a } = o; return a; }\n",
    )
    .unwrap();

    common::comply()
        .arg("--fix")
        .arg(root)
        .assert()
        .stderr(predicate::str::contains("comply: ran 1 fixer(s)"));
    let ts = fs::read_to_string(root.join("a.ts")).unwrap();
    assert!(ts.contains("const { a } = o"), "{ts}");
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
    shear_run("echo 'Error: Metadata error: `cargo metadata` exited with an error:'\nexit 2\n")
        .code(2)
        .stderr(predicate::str::contains("cargo shear could not check"))
        .stderr(predicate::str::contains("It exited with 2"))
        .stderr(predicate::str::contains("Metadata error"));
}

// Exit 1 announces a finding.
#[cfg(unix)]
#[test]
fn cargo_shear_that_exits_1_without_a_finding_is_an_error() {
    shear_run("echo '{\"findings\":[]}'\necho 'out of memory' >&2\nexit 1\n")
        .code(2)
        .stderr(predicate::str::contains("cargo shear could not check"))
        .stderr(predicate::str::contains("It exited with 1"))
        .stderr(predicate::str::contains("out of memory"));
}

/// Runs comply where `cargo shear` is the shell lines `shear`.
#[cfg(unix)]
fn shear_run(shear: &str) -> assert_cmd::assert::Assert {
    use std::os::unix::fs::PermissionsExt;
    let dir = rust_crate(&[("src/lib.rs", "pub fn f() {}\n")]);
    let bin = TempDir::new().unwrap();
    let script = bin.path().join("cargo-shear");
    let version = "case \"$*\" in *--version*) echo 'cargo-shear 1.14.0'; exit 0;; esac";
    fs::write(&script, format!("#!/bin/sh\n{version}\n{shear}")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin.path().to_path_buf()];
    dirs.extend(std::env::split_paths(&path));
    common::comply()
        .current_dir(dir.path())
        .env("PATH", std::env::join_paths(dirs).unwrap())
        .arg(".")
        .assert()
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

/// Crate `a`, and `b` which depends on it.
/// `b` holds a `clippy::len_zero`.
fn dependent_crates(files: &[(&str, &str)]) -> TempDir {
    let mut tree = vec![
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
        ),
        (
            "a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("a/src/lib.rs", "pub const KEEP: u8 = 1;\n"),
        (
            "b/Cargo.toml",
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\na = { path = \"../a\" }\n",
        ),
        (
            "b/src/lib.rs",
            "pub fn is_none(v: &[u8]) -> bool {\n    a::KEEP == 1 && v.len() == 0\n}\n",
        ),
    ];
    tree.extend_from_slice(files);
    let dir = rust_crate(&tree);
    fs::write(dir.path().join(".gitignore"), "target/\n").unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "init"]);
    dir
}

/// Comply over the staged lines `append` adds to `b`.
fn staged_run(root: &std::path::Path, append: &str) -> assert_cmd::assert::Assert {
    let lib = root.join("b/src/lib.rs");
    let source = fs::read_to_string(&lib).unwrap();
    fs::write(&lib, format!("{source}{append}")).unwrap();
    git(root, &["add", "-A"]);
    common::comply()
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .args(["--staged", "--diff-only"])
        .assert()
}

const NEW_LEN_ZERO: &str =
    "\n/// Holds nothing.\n#[must_use]\npub fn is_bare(v: &[u8]) -> bool {\n    v.len() == 0\n}\n";

// A build script that panics warns about its own `panic!`.
// That warning excused the failure of the whole run.
#[test]
fn clippy_stopped_by_a_build_script_that_panics_is_an_error() {
    let dir = dependent_crates(&[(
        "b/build.rs",
        "//! Generates.\nfn main() {\n    panic!(\"protoc not found\")\n}\n",
    )]);
    staged_run(dir.path(), NEW_LEN_ZERO)
        .code(2)
        .stdout(predicate::str::contains("all clear").not())
        .stderr(predicate::str::contains("protoc not found"));
}

// So did any warning of another crate.
#[test]
fn clippy_stopped_by_a_killed_build_script_is_an_error() {
    let dir = dependent_crates(&[
        ("a/src/lib.rs", "use std::fmt;\npub const KEEP: u8 = 1;\n"),
        (
            "b/build.rs",
            "//! Dies.\nfn main() {\n    std::process::abort()\n}\n",
        ),
    ]);
    staged_run(dir.path(), NEW_LEN_ZERO)
        .code(2)
        .stdout(predicate::str::contains("all clear").not())
        .stderr(predicate::str::contains("cargo clippy could not compile"));
}

/// `dependent_crates`, where `a` denies `clippy::len_zero` and breaks it.
fn len_zero_denied_in_a() -> TempDir {
    dependent_crates(&[(
        "a/src/lib.rs",
        "#![deny(clippy::len_zero)]\npub const KEEP: u8 = 1;\n\
         pub fn is_old(v: &[u8]) -> bool {\n    v.len() == 0\n}\n",
    )])
}

// `a` is not staged. Its denied lint stops `b`.
// The capped pass lints `b` past it.
#[test]
fn clean_lines_past_a_denied_lint_of_another_crate_pass() {
    let dir = len_zero_denied_in_a();
    staged_run(dir.path(), "\n/// More.\npub const MORE: u8 = 2;\n")
        .code(0)
        .stderr(predicate::str::contains("did not lint").not());
}

#[test]
fn findings_past_a_denied_lint_of_another_crate_are_reported() {
    let dir = len_zero_denied_in_a();
    staged_run(dir.path(), NEW_LEN_ZERO)
        .code(1)
        .stdout(predicate::str::contains("b/src/lib.rs:8:5"))
        .stdout(predicate::str::contains("clippy::len_zero"))
        .stdout(predicate::str::contains("a/src/lib.rs").not());
}

// clippy denies `x < 0` by default.
// In `a` or `b`, staged lines get one verdict.
#[test]
fn a_denied_lint_weighs_the_same_in_any_crate() {
    let low = "pub fn is_low(x: u32) -> bool {\n    x < 0\n}\n";
    for (path, source) in [
        ("a/src/lib.rs", format!("pub const KEEP: u8 = 1;\n{low}")),
        (
            "b/src/lib.rs",
            format!(
                "pub fn is_none(v: &[u8]) -> bool {{\n    a::KEEP == 1 && v.len() == 0\n}}\n{low}"
            ),
        ),
    ] {
        let dir = dependent_crates(&[(path, source.as_str())]);
        let output = staged_run(dir.path(), NEW_LEN_ZERO).get_output().clone();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let findings = stdout
            .lines()
            .filter(|line| line.contains("clippy::len_zero"))
            .count();
        assert_eq!(output.status.code(), Some(1), "{path}: {stderr}");
        assert_eq!(findings, 1, "{path}: {stdout}");
    }
}

/// The workspace of `a` alone, holding `a_lib`.
/// Crate `x`, at `tools/x`, is no member.
fn crate_outside_the_workspace(a_lib: &str) -> TempDir {
    rust_crate(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\"]\nresolver = \"2\"\n",
        ),
        (
            "a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("a/src/lib.rs", a_lib),
        (
            "tools/x/Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "tools/x/src/lib.rs",
            "pub fn is_loose(v: &[u8]) -> bool {\n    v.len() == 0\n}\n",
        ),
    ])
}

// No pass lints `x`. A full scan names it.
// A denied lint of `a` changes nothing.
#[test]
fn a_full_scan_names_the_crates_outside_the_workspace() {
    for a_lib in [
        "/// Kept.\npub const KEEP: u8 = 1;\n",
        "#![deny(clippy::len_zero)]\npub fn is_old(v: &[u8]) -> bool {\n    v.len() == 0\n}\n",
    ] {
        let dir = crate_outside_the_workspace(a_lib);
        let output = common::comply()
            .current_dir(dir.path())
            .env("CARGO_TARGET_DIR", dir.path().join("target"))
            .arg(".")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_ne!(output.status.code(), Some(2), "{stderr}");
        assert!(
            stderr.contains("1 crate(s) outside the cargo workspace"),
            "{stderr}"
        );
        assert!(stderr.contains("tools/x/src/lib.rs"), "{stderr}");
    }
}

// An index mode cannot pass `x` unlinted.
// It refuses, naming the crate and its files.
#[test]
fn index_modes_refuse_a_crate_outside_the_workspace() {
    let dir = crate_outside_the_workspace("/// Kept.\npub const KEEP: u8 = 1;\n");
    let root = dir.path();
    fs::write(root.join(".gitignore"), "target/\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "init"]);
    let append = |path: &str, line: &str| {
        let file = root.join(path);
        let source = fs::read_to_string(&file).unwrap();
        fs::write(&file, format!("{source}{line}")).unwrap();
        git(root, &["add", "-A"]);
    };
    append("tools/x/src/lib.rs", "/// More.\npub const MORE: u8 = 2;\n");
    git(root, &["commit", "-qm", "more"]);
    append("tools/x/src/lib.rs", "/// Most.\npub const MOST: u8 = 3;\n");
    append("a/src/lib.rs", "/// Less.\npub const LESS: u8 = 0;\n");

    for args in [
        &["--range", "HEAD~1", "HEAD"][..],
        &["--staged"][..],
        &["--staged", "--diff-only"][..],
    ] {
        let output = common::comply()
            .current_dir(root)
            .env("CARGO_TARGET_DIR", root.join("target"))
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {stderr}");
        assert!(!stdout.contains("all clear"), "{args:?}: {stdout}");
        assert!(stderr.contains("x: "), "{args:?}: {stderr}");
        assert!(stderr.contains("tools/x/src/lib.rs"), "{args:?}: {stderr}");
    }
}

// No crate holds a loose Rust file.
// An index mode refuses it; a full scan notes it.
#[test]
fn a_loose_rust_file_is_refused_in_an_index_mode() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("loose.rs"), "/// One.\npub const ONE: u8 = 1;\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);

    let staged = common::comply()
        .current_dir(root)
        .arg("--staged")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&staged.stderr);
    assert_eq!(staged.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains(".complyignore"), "{stderr}");
    assert!(stderr.contains("loose.rs"), "{stderr}");

    let full = common::comply()
        .current_dir(root)
        .arg(".")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&full.stderr);
    assert_ne!(full.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains(".complyignore"), "{stderr}");
}

// `a` reaches `shared/s.rs` through `#[path]`; no Cargo.toml sits above it.
// Changing it alone still lints it, through `a`.
#[test]
fn a_file_a_crate_reaches_through_path_is_linted_with_it() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    for (path, content) in [
        (
            "a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
        ),
        (
            "a/src/main.rs",
            "//! Fixture.\n#[path = \"../../shared/s.rs\"]\nmod s;\nfn main() {\n    println!(\"{}\", s::ONE);\n}\n",
        ),
        ("shared/s.rs", "/// One.\npub const ONE: u8 = 1;\n"),
        (".gitignore", "target/\n"),
    ] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "init"]);
    fs::write(
        root.join("shared/s.rs"),
        "/// Two.\npub const ONE: u8 = 2;\n",
    )
    .unwrap();
    git(root, &["add", "shared/s.rs"]);
    // The full scan also judges `a`, whose fixture draws findings.
    for (args, code) in [(&["--staged"][..], 0), (&["."][..], 1)] {
        let output = common::comply()
            .current_dir(root)
            .env("CARGO_TARGET_DIR", root.join("target"))
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(code),
            "{args:?}: {stdout}{stderr}"
        );
        assert!(!stderr.contains("shared/s.rs"), "{args:?}: {stderr}");
        assert!(!stdout.contains("shared/s.rs"), "{args:?}: {stdout}");
    }
}

// No `mod` item reads `src/orphan.rs`, so clippy never sees it.
// An index mode refuses it; a full scan notes it.
#[test]
fn a_rust_file_no_target_compiles_is_unanalyzed() {
    let dir = rust_crate(&[
        (
            "src/lib.rs",
            "//! Fixture.\n#[path = \"extra/x.rs\"]\nmod x;\n/// Kept.\npub const KEEP: u8 = x::ONE;\n",
        ),
        ("src/extra/x.rs", "/// One.\npub const ONE: u8 = 1;\n"),
        ("src/orphan.rs", "/// Two.\npub const TWO: u8 = 2;\n"),
    ]);
    let root = dir.path();
    fs::write(root.join(".gitignore"), "target/\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    for (args, refused) in [(&["--staged"][..], true), (&["."][..], false)] {
        let output = common::comply()
            .current_dir(root)
            .env("CARGO_TARGET_DIR", root.join("target"))
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code() == Some(2),
            refused,
            "{args:?}: {stderr}"
        );
        if refused {
            assert!(!stdout.contains("all clear"), "{args:?}: {stdout}");
        }
        assert!(stderr.contains("src/orphan.rs"), "{args:?}: {stderr}");
        assert!(!stderr.contains("extra/x.rs"), "{args:?}: {stderr}");
        // Remedies: a cfg this host disables, or .complyignore.
        assert!(stderr.contains("#[cfg(target_os = "), "{args:?}: {stderr}");
        assert!(stderr.contains(".complyignore"), "{args:?}: {stderr}");
    }
}

// `include_str!` lists a file in the dep-info as text, not code.
#[test]
fn a_rust_file_only_include_str_reads_is_unanalyzed() {
    let read = "//! Reads.\n#[test]\nfn reads() {\n    \
                assert!(!include_str!(\"fixtures/input.rs\").is_empty());\n}\n";
    let dir = rust_crate(&[
        ("src/lib.rs", "//! Fixture.\n"),
        ("tests/read.rs", read),
        (
            "tests/fixtures/input.rs",
            "/// One.\npub const ONE: u8 = 1;\n",
        ),
    ]);
    let root = dir.path();
    fs::write(root.join(".gitignore"), "target/\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "test"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "base"]);
    let input = "/// Two.\npub const ONE: u8 = 2;\n";
    fs::write(root.join("tests/fixtures/input.rs"), input).unwrap();
    git(root, &["add", "-A"]);
    let output = common::comply()
        .current_dir(root)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .arg("--staged")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stdout}{stderr}");
    assert!(stderr.contains("tests/fixtures/input.rs"), "{stderr}");
}

// A fixtures crate nested in `a`, and excluded.
// A full scan still lints `a`.
#[test]
fn a_nested_excluded_fixtures_crate_keeps_a_full_scan() {
    let dir = rust_crate(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\"]\nexclude = [\"a/tests/fixtures/x\"]\nresolver = \"2\"\n",
        ),
        (
            "a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "a/src/lib.rs",
            "/// Empty.\n#[must_use]\npub fn is_none(v: &[u8]) -> bool {\n    v.len() == 0\n}\n",
        ),
        (
            "a/tests/fixtures/x/Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "a/tests/fixtures/x/src/lib.rs",
            "/// Kept.\npub const KEEP: u8 = 1;\n",
        ),
    ]);
    common::comply()
        .current_dir(dir.path())
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .arg(".")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("a/src/lib.rs:4:5"))
        .stdout(predicate::str::contains("clippy::len_zero"));
}

/// Comply over a crate holding `files`, with its clippy findings.
fn clippy_findings(files: &[(&str, &str)]) -> (Option<i32>, Vec<String>, String) {
    let dir = rust_crate(files);
    let output = common::comply()
        .current_dir(dir.path())
        .env("CARGO_TARGET_DIR", dir.path().join("target"))
        .arg(".")
        .output()
        .unwrap();
    let findings = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("clippy::len_zero"))
        .map(str::to_owned)
        .collect();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status.code(), findings, stderr)
}

const LEN_ZERO: &str = "pub fn is_none(v: &[u8]) -> bool {\n    v.len() == 0\n}\n";

#[test]
fn denied_clippy_lint_stays_a_finding_at_every_level() {
    let manifest = |lints: &str| {
        format!(
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [workspace]\n{lints}"
        )
    };
    for (attribute, lints) in [
        ("", ""),
        ("#![deny(clippy::len_zero)]\n", ""),
        ("#![forbid(clippy::len_zero)]\n", ""),
        ("#![deny(warnings)]\n", ""),
        ("", "\n[lints.clippy]\nlen_zero = \"deny\"\n"),
        ("", "\n[lints.clippy]\nlen_zero = \"forbid\"\n"),
    ] {
        let (code, findings, stderr) = clippy_findings(&[
            ("Cargo.toml", &manifest(lints)),
            ("src/lib.rs", &format!("{attribute}{LEN_ZERO}")),
            (
                "src/main.rs",
                "fn main() {\n    println!(\"{}\", fixture::is_none(&[]));\n}\n",
            ),
        ]);
        assert_eq!(code, Some(1), "{attribute}{lints}: {stderr}");
        assert_eq!(findings.len(), 1, "{attribute}{lints}: {findings:?}");
    }
}

// A denied lint stops the library the binary needs.
#[test]
fn denied_clippy_lint_does_not_hide_the_findings_behind_it() {
    let (code, findings, stderr) = clippy_findings(&[
        (
            "src/lib.rs",
            &format!("#![deny(clippy::len_zero)]\n{LEN_ZERO}"),
        ),
        (
            "src/main.rs",
            "fn main() {\n    let v: Vec<u8> = Vec::new();\n    \
             println!(\"{} {}\", v.len() == 0, fixture::is_none(&v));\n}\n",
        ),
    ]);
    assert_eq!(code, Some(1), "{stderr}");
    assert_eq!(findings.len(), 2, "{findings:?}");
}

// Capping the levels must not turn rustc's errors into silence.
#[test]
fn rustc_lint_denied_behind_a_denied_clippy_lint_is_an_error() {
    for source in [
        "#![deny(unused_variables)]\nfn main() {\n    let unread = 1;\n}\n",
        "#![deny(warnings)]\nuse fixture::is_none;\nfn main() {}\n",
    ] {
        let (code, _, stderr) = clippy_findings(&[
            (
                "src/lib.rs",
                &format!("#![deny(clippy::len_zero)]\n{LEN_ZERO}"),
            ),
            ("src/main.rs", source),
        ]);
        assert_eq!(code, Some(2), "{stderr}");
        assert!(stderr.contains("unused"), "{stderr}");
    }
}

/// `fixture` denies its unused dependency `idle`.
/// `members` joins `idle` in the workspace.
fn idle_dependency_manifest(members: &str) -> String {
    format!(
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [lints.cargo]\nunused_dependencies = \"deny\"\n\n\
         [dependencies]\nidle = {{ path = \"idle\" }}\n\n\
         [workspace]\nmembers = [\"idle\"{members}]\n"
    )
}

const IDLE: [(&str, &str); 2] = [
    (
        "idle/Cargo.toml",
        "[package]\nname = \"idle\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    ),
    ("idle/src/lib.rs", "//! Unused.\n"),
];

// From 1.100, cargo fails a run whose crates all built.
#[test]
fn denied_cargo_lint_does_not_hide_the_findings() {
    let manifest = idle_dependency_manifest("");
    for attribute in ["", "#![deny(clippy::len_zero)]\n"] {
        let source = format!("{attribute}{LEN_ZERO}");
        let mut files = vec![("Cargo.toml", manifest.as_str()), ("src/lib.rs", &source)];
        files.extend(IDLE);
        let (code, findings, stderr) = clippy_findings(&files);
        assert_eq!(code, Some(1), "{attribute}: {stderr}");
        assert_eq!(findings.len(), 1, "{attribute}: {findings:?}");
    }
}

#[test]
fn denied_cargo_lint_excuses_no_other_failure() {
    let manifest = idle_dependency_manifest(", \"dies\"");
    let mut files = vec![
        ("Cargo.toml", manifest.as_str()),
        ("src/lib.rs", LEN_ZERO),
        (
            "dies/Cargo.toml",
            "[package]\nname = \"dies\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("dies/src/lib.rs", "//! Never built.\n"),
        (
            "dies/build.rs",
            "fn main() {\n    std::process::abort()\n}\n",
        ),
    ];
    files.extend(IDLE);
    let (code, _, stderr) = clippy_findings(&files);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("custom build command"), "{stderr}");
}

// A cargo config may deny warnings, or hide them.
#[test]
fn cargo_build_warnings_setting_does_not_change_the_findings() {
    for setting in ["deny", "allow"] {
        let (code, findings, stderr) = clippy_findings(&[
            (
                ".cargo/config.toml",
                &format!("[build]\nwarnings = \"{setting}\"\n"),
            ),
            ("src/lib.rs", LEN_ZERO),
        ]);
        assert_eq!(code, Some(1), "{setting}: {stderr}");
        assert_eq!(findings.len(), 1, "{setting}: {findings:?}");
    }
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

/// oxlint 1.90.0's own output on `OXLINT_190_SOURCES`, as comply ran it.
/// One plain pass, one `--type-aware` pass, both verbatim.
const OXLINT_190_PLAIN: &str = include_str!("fixtures/oxlint-1.90.0/plain.json");
const OXLINT_190_TYPE_AWARE: &str = include_str!("fixtures/oxlint-1.90.0/type-aware.json");

/// The files oxlint 1.90.0 linted for those outputs.
const OXLINT_190_SOURCES: &[(&str, &str)] = &[
    ("package.json", "{\"name\":\"fx\",\"private\":true}\n"),
    (
        "src/app.ts",
        "export async function load(): Promise<number> {\n  return 1;\n}\n\n\
         export function fire(): void {\n  load();\n}\n\n\
         export async function wait(): Promise<number> {\n  const n = 2;\n  return await n;\n}\n\n\
         export function wrap(): Promise<number> {\n  return load();\n}\n\n\
         export const pass = (): Promise<number> => load();\n\n\
         export const list: Array<number> = [1];\nexport const relay = () => load();\n",
    ),
    (
        "src/app.test.ts",
        "import { load } from \"./app\";\n\nexport const shared = 1;\n\n\
         it(\"loads\", async () => {\n  const n = 3;\n  await n;\n  expect(await load()).toBe(1);\n});\n",
    ),
    (
        "src/tx-test.ts",
        "export const helper = 1;\n\ntest(\"wrapped\", () => {\n  expect(helper).toBe(1);\n});\n",
    ),
    (
        "src/state.ts",
        "export let counter = 0;\n\n\
         export function sum(values: number[]): number[] {\n  values.forEach((value) => {\n    counter += value;\n  });\n  \
         return values.reduce((acc, value) => [...acc, value], [] as number[]);\n}\n\n\
         export function settle(): Promise<number> {\n  \
         return new Promise((done, fail) => (counter > 0 ? done(counter) : fail(new Error(\"none\"))));\n}\n",
    ),
    (
        "src/view.tsx",
        "import { useEffect } from \"react\";\n\n\
         export function View({ id }: { id: string }) {\n  useEffect(() => {\n    console.log(id);\n  }, []);\n  \
         return <div className={\"view\"}>{id}</div>;\n}\n",
    ),
];

/// Stdout of `comply <args>` on `OXLINT_190_SOURCES` and `comply_toml`.
/// oxlint replays its 1.90.0 output.
#[cfg(unix)]
fn oxlint_190_run(args: &[&str], comply_toml: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    for (path, content) in OXLINT_190_SOURCES {
        let file = dir.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, content).unwrap();
    }
    fs::write(dir.path().join("comply.toml"), comply_toml).unwrap();
    let bin = TempDir::new().unwrap();
    fs::write(bin.path().join("plain.json"), OXLINT_190_PLAIN).unwrap();
    fs::write(bin.path().join("type-aware.json"), OXLINT_190_TYPE_AWARE).unwrap();
    let script = bin.path().join("oxlint");
    let replay = format!(
        "#!/bin/sh\ncase \"$*\" in\n*--version*) echo 'Version: 1.90.0'; exit 0;;\n\
         *--type-aware*) cat '{0}/type-aware.json';;\n*) cat '{0}/plain.json';;\nesac\nexit 1\n",
        bin.path().display()
    );
    fs::write(&script, replay).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin.path().to_path_buf()];
    dirs.extend(std::env::split_paths(&path));
    let output = common::comply()
        .current_dir(dir.path())
        .env("PATH", std::env::join_paths(dirs).unwrap())
        .args(args)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap()
}

/// A directory whose `oxlint` reports version 1.90.0.
/// Every other call runs the shell `lint`.
#[cfg(unix)]
fn stub_oxlint(lint: &str) -> TempDir {
    use std::os::unix::fs::PermissionsExt;
    let bin = TempDir::new().unwrap();
    let script = bin.path().join("oxlint");
    let body = format!(
        "#!/bin/sh\ncase \"$*\" in\n*--version*) echo 'Version: 1.90.0'; exit 0;;\nesac\n{lint}\n"
    );
    fs::write(&script, body).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// `comply --staged` refuses the batch; `comply .` notes it.
/// The batch is `src/a.ts`, and oxlint runs the shell `lint`.
/// Both stderr hold each of `why`.
#[cfg(unix)]
fn assert_unanalyzed_batch(lint: &str, envs: &[(&str, &std::path::Path)], why: &[&str]) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("package.json"),
        "{\"name\":\"fx\",\"private\":true}\n",
    )
    .unwrap();
    fs::write(root.join("src/a.ts"), "export const A = 1;\n").unwrap();
    fs::write(root.join("tsconfig.json"), "{}\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    let bin = stub_oxlint(lint);
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin.path().to_path_buf()];
    dirs.extend(std::env::split_paths(&path));
    let path = std::env::join_paths(dirs).unwrap();
    let run = |args: &[&str]| {
        let mut comply = common::comply();
        comply.current_dir(root).env("PATH", &path).args(args);
        for (name, value) in envs {
            comply.env(name, value);
        }
        comply.output().unwrap()
    };
    // oxlint may hang: both scans wait on it at once.
    let (staged, full) = std::thread::scope(|scope| {
        let staged = scope.spawn(|| run(&["--staged"]));
        let full = scope.spawn(|| run(&["."]));
        (staged.join().unwrap(), full.join().unwrap())
    });
    for (scan, output, refused) in [("--staged", staged, true), (".", full, false)] {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code() == Some(2), refused, "{scan}: {stderr}");
        if refused {
            assert!(!stdout.contains("all clear"), "{scan}: {stdout}");
        }
        for needle in why.iter().chain(&["src/a.ts"]) {
            assert!(
                stderr.contains(needle),
                "{scan}: {needle} missing:\n{stderr}"
            );
        }
    }
}

// oxlint runs past its 45s budget on the plain batch.
// An index mode refuses it; a full scan notes it.
#[cfg(unix)]
#[test]
fn a_batch_oxlint_ran_past_its_budget_on_is_unanalyzed() {
    let hang = "case \"$*\" in\n*--type-aware*) echo '{\"diagnostics\": []}';;\n\
                *) exec sleep 120;;\nesac";
    assert_unanalyzed_batch(
        hang,
        &[],
        &["oxlint ran past 45s", "rerun", ".complyignore"],
    );
}

// tsgolint panics on the batch; no diagnostic comes back.
// The message quotes the panic and the tsgolint version.
#[cfg(unix)]
#[test]
fn a_batch_tsgolint_panicked_on_is_unanalyzed() {
    let package = TempDir::new().unwrap();
    fs::create_dir_all(package.path().join("bin")).unwrap();
    fs::write(
        package.path().join("package.json"),
        "{\"name\":\"oxlint-tsgolint\",\"version\":\"7.0.2005\"}\n",
    )
    .unwrap();
    let tsgolint = package.path().join("bin/tsgolint");
    fs::write(&tsgolint, "#!/bin/sh\n").unwrap();
    let panic = "echo 'panic: Unhandled case in Node.MemberList: KindModuleDeclaration' >&2\n\
                 echo 'goroutine 7 [running]:' >&2\n\
                 echo 'github.com/typescript-eslint/tsgolint/internal/rules' >&2\nexit 1";
    assert_unanalyzed_batch(
        panic,
        &[("OXLINT_TSGOLINT_PATH", &tsgolint)],
        &[
            "tsgolint 7.0.2005",
            "panic: Unhandled case in Node.MemberList: KindModuleDeclaration",
        ],
    );
}

// oxlint says it linted none of the batch.
// An ignore file it read left the file out.
#[cfg(unix)]
#[test]
fn a_batch_oxlint_left_out_is_unanalyzed() {
    let none = "echo '{\"diagnostics\": [], \"number_of_files\": 0}'";
    let why = [
        "oxlint linted only 0 of the 1 file(s)",
        "/.eslintignore",
        ".complyignore",
    ];
    assert_unanalyzed_batch(none, &[], &why);
}

// `.eslintignore` drops `gen.ts` and `out/` from a full scan.
// A git file scope drops them too, alone or not.
#[test]
fn a_git_scope_drops_the_files_eslintignore_drops() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let flawed = "var x = 1;\ndebugger;\nexport { x };\n";
    fs::create_dir_all(root.join("out")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("package.json"),
        "{\"name\":\"fx\",\"private\":true}\n",
    )
    .unwrap();
    fs::write(root.join(".eslintignore"), "gen.ts\nout/\n").unwrap();
    fs::write(root.join("tsconfig.json"), "{}\n").unwrap();
    fs::write(root.join("gen.ts"), flawed).unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    let scan = |args: &[&str]| {
        let output = common::comply()
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        (output.status.code(), stdout, stderr)
    };
    let (code, stdout, stderr) = scan(&["--staged"]);
    assert_eq!(code, Some(0), "alone: {stdout}{stderr}");

    fs::write(root.join("out/y.ts"), flawed).unwrap();
    fs::write(root.join("src/a.ts"), "export const A = 1;\n").unwrap();
    git(root, &["add", "-A"]);
    for args in [&["--staged"][..], &["."][..]] {
        let (code, stdout, stderr) = scan(args);
        assert_eq!(code, Some(0), "{args:?}: {stdout}{stderr}");
    }
}

/// A misused promise: tsgolint's `no-misused-promises` flags line 6.
const MISUSED_PROMISE: &str = "export async function load(): Promise<number> {
  return 2;
}

export function check(): boolean {
  if (load()) {
    return true;
  }
  return false;
}
";

/// A repo of two TypeScript packages, `pkg-a` and `pkg-b`, committed.
/// The sidecar rules are off; tsgolint still runs.
fn two_package_repo(root: &std::path::Path) {
    for package in ["pkg-a", "pkg-b"] {
        let dir = root.join(package);
        fs::create_dir_all(&dir).unwrap();
        let manifest = format!("{{\"name\":\"{package}\",\"private\":true}}\n");
        fs::write(dir.join("package.json"), manifest).unwrap();
        let tsconfig = "{\"compilerOptions\":{\"strict\":true}}\n";
        fs::write(dir.join("tsconfig.json"), tsconfig).unwrap();
    }
    fs::write(root.join("pkg-a/a.ts"), "export const A = 1;\n").unwrap();
    fs::write(root.join("pkg-b/base.ts"), "export const B = 1;\n").unwrap();
    common::without_type_aware_rules(root);
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "test"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "base"]);
}

/// The code, stdout and stderr of `comply --staged` in `root`.
fn staged_scan(root: &std::path::Path) -> (Option<i32>, String, String) {
    let output = common::comply()
        .current_dir(root)
        .arg("--staged")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status.code(), stdout, stderr)
}

// oxlint names each file as passed, relative to the scan.
// A partial scan must still keep its type-aware findings.
#[test]
fn a_staged_scan_keeps_the_type_aware_findings() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    two_package_repo(root);
    fs::write(root.join("pkg-b/b.ts"), MISUSED_PROMISE).unwrap();
    git(root, &["add", "pkg-b/b.ts"]);
    let (code, stdout, stderr) = staged_scan(root);
    assert_eq!(code, Some(1), "{stdout}{stderr}");
    let finding = "pkg-b/b.ts:6:7: error [no-misused-promises]";
    assert!(stdout.contains(finding), "{stdout}{stderr}");
}

// `pkg-a` sorts first; `pkg-b` still gets its own program.
#[test]
fn a_staged_scan_type_checks_every_changed_package() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    two_package_repo(root);
    fs::write(root.join("pkg-a/a.ts"), "export const A = 2;\n").unwrap();
    fs::write(root.join("pkg-b/b.ts"), MISUSED_PROMISE).unwrap();
    git(root, &["add", "-A"]);
    let (code, stdout, stderr) = staged_scan(root);
    assert_eq!(code, Some(1), "{stdout}{stderr}");
    let finding = "pkg-b/b.ts:6:7: error [no-misused-promises]";
    assert!(stdout.contains(finding), "{stdout}{stderr}");
}

// The walk prunes these directories.
// A git scope still lists the file.
// Its package's program takes it in for tsgolint.
#[test]
fn a_staged_file_the_walk_prunes_is_type_checked() {
    for pruned in ["vendor", "build", ".cache"] {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        two_package_repo(root);
        let file = format!("pkg-b/{pruned}/v.ts");
        fs::create_dir_all(root.join("pkg-b").join(pruned)).unwrap();
        fs::write(root.join(&file), MISUSED_PROMISE).unwrap();
        git(root, &["add", "-f", &file]);
        let (code, stdout, stderr) = staged_scan(root);
        assert_eq!(code, Some(1), "{pruned}: {stdout}{stderr}");
        let finding = format!("{file}:6:7: error [no-misused-promises]");
        assert!(stdout.contains(&finding), "{pruned}: {stdout}{stderr}");
    }
}

// oxlint 1.90.0 names plugins `typescript(…)`, `import(…)`, `jest(…)`.
// Each finding still carries its comply rule id.
#[cfg(unix)]
#[test]
fn oxlint_190_findings_carry_the_comply_rule_id() {
    let out = oxlint_190_run(&["."], "");
    for finding in [
        "src/app.ts:6:3: error [no-floating-promises]",
        "src/app.ts:20:20: error [typescript/array-type]",
        "src/state.ts:1:8: error [import/no-mutable-exports]",
        "src/state.ts:4:10: error [unicorn/no-array-for-each]",
        "src/state.ts:11:23: error [promise/param-names]",
        "src/view.tsx:5:17: error [react-exhaustive-deps]",
        "src/view.tsx:7:26: error [react-jsx-curly-brace-presence]",
        "src/app.test.ts:3:14: error [jest-no-export]",
    ] {
        assert!(out.contains(finding), "{finding} missing:\n{out}");
    }
    // No finding keeps the `plugin(rule)` code oxlint wrote.
    for line in out.lines() {
        let id = line
            .split_once(" [")
            .and_then(|(_, rest)| rest.split_once(']'));
        assert!(!id.is_some_and(|(id, _)| id.contains('(')), "{line}");
    }
}

// Each rule filter reads the comply rule id.
// Tests may await plain values.
// Pass-through arrows need no `async`.
// jest rules check `.test.` and `.spec.` files.
// `it` alone is consistent.
#[cfg(unix)]
#[test]
fn oxlint_190_findings_go_through_the_rule_filters() {
    let out = oxlint_190_run(&["."], "");
    for kept in [
        "src/app.ts:11:16: error [await-thenable]",
        "src/app.ts:14:1: error [promise-function-async]",
        "src/app.test.ts:3:14: error [jest-no-export]",
    ] {
        assert!(out.contains(kept), "{kept} missing:\n{out}");
    }
    // A dropped finding shows under no code, old or new.
    for (place, rule) in [
        ("src/app.test.ts:7:9:", "await-thenable"),
        ("src/app.ts:21:22:", "promise-function-async"),
        ("src/tx-test.ts:1:14:", "no-export"),
        ("src/app.test.ts:5:1:", "consistent-test-it"),
    ] {
        let shown = out
            .lines()
            .any(|line| line.contains(place) && line.contains(rule));
        assert!(!shown, "{place} {rule} kept:\n{out}");
    }
}

// comply.toml turns a delegated rule off by its comply id.
#[cfg(unix)]
#[test]
fn oxlint_190_findings_follow_comply_toml() {
    let toml = "[rules.return-await]\ndisabled = true\n\n\
                [overrides.\"src/view.tsx\"]\ndisable = [\"react-jsx-curly-brace-presence\"]\n";
    let out = oxlint_190_run(&["."], toml);
    for (finding, kept) in [
        ("src/app.ts:6:3: error [no-floating-promises]", true),
        ("src/app.ts:11:10: error [return-await]", false),
        (
            "src/view.tsx:7:26: error [react-jsx-curly-brace-presence]",
            false,
        ),
    ] {
        assert_eq!(out.contains(finding), kept, "{finding}:\n{out}");
    }
}

// `comply rules` keeps the findings of the rules it names.
#[cfg(unix)]
#[test]
fn oxlint_190_findings_answer_comply_rules() {
    let out = oxlint_190_run(&["rules", "no-floating-promises", "."], "");
    assert!(
        out.contains("src/app.ts:6:3: error [no-floating-promises]"),
        "{out}"
    );
}
