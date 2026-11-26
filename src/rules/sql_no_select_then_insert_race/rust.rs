//! sql-no-select-then-insert-race — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::RUST_STRING_KINDS;

/// Owned snapshot of one SQL-bearing string literal. Stores the text plus the
/// byte range so we can later emit a diagnostic anchored at the same span as
/// `Diagnostic::at_node` would have produced.
struct Collected {
    text: String,
    line: usize,
    column: usize,
    byte_start: usize,
    byte_len: usize,
    /// The start of the enclosing function: one flow of execution.
    scope: usize,
}

#[derive(Default)]
struct State {
    nodes: Vec<Collected>,
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(RUST_STRING_KINDS)
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::<State>::default())
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Ok(text) = node.utf8_text(ctx.source.as_bytes()) else {
            return;
        };
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        let is_sql = super::extract_select_from_table(text).is_some()
            || super::extract_insert_into_table(text).is_some();
        if !is_sql {
            return;
        }
        // Items at module level never run in sequence.
        let Some(function) = crate::rules::rust_helpers::enclosing_fn(node) else {
            return;
        };
        // A test hands its literals to independent calls.
        if crate::rules::rust_helpers::is_test_only(node, ctx) {
            return;
        }
        let pos = node.start_position();
        let range = node.byte_range();
        state.nodes.push(Collected {
            text: text.to_string(),
            line: pos.row + 1,
            column: pos.column + 1,
            byte_start: range.start,
            byte_len: range.len(),
            scope: function.start_byte(),
        });
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast::<State>().ok()) else {
            return;
        };
        let nodes = &state.nodes;
        let mut reported: Vec<usize> = Vec::new();
        for (i, sel) in nodes.iter().enumerate() {
            let Some(sel_table) = super::extract_select_from_table(&sel.text) else {
                continue;
            };
            for (offset, ins) in nodes[i + 1..].iter().enumerate() {
                let ins_index = i + 1 + offset;
                if ins.scope != sel.scope {
                    continue;
                }
                let Some(ins_table) = super::extract_insert_into_table(&ins.text) else {
                    continue;
                };
                if ins_table != sel_table {
                    continue;
                }
                if super::has_on_conflict(&ins.text) {
                    break;
                }
                if reported.contains(&ins_index) {
                    break;
                }
                reported.push(ins_index);
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: ins.line,
                    column: ins.column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "SELECT then INSERT on `{sel_table}` is a TOCTOU race — use `INSERT ... ON CONFLICT`."
                    ),
                    severity: Severity::Error,
                    span: Some((ins.byte_start, ins.byte_len)),
                });
                break;
            }
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_select_then_insert_same_table() {
        let src = r#"fn f() { let a = "SELECT id FROM user WHERE email = $1"; let b = "INSERT INTO user (email) VALUES ($1)"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_on_conflict() {
        let src = r#"fn f() { let a = "SELECT id FROM user WHERE email = $1"; let b = "INSERT INTO user (email) VALUES ($1) ON CONFLICT (email) DO NOTHING"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_select_advisory_lock_without_from_issue_1451() {
        // sqlx-postgres testing/mod.rs: `select pg_advisory_xact_lock(...)` is a
        // lock acquisition, not a data read — no FROM clause, so it cannot be the
        // read half of a SELECT→INSERT race.
        let src = r##"fn setup() {
            conn.execute(r#"select pg_advisory_xact_lock(8318549251334697844);"#);
            query(r#"insert into _sqlx_test.databases(db_name, test_path) values ($1, $2)"#);
        }"##;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_format_placeholder_table_issue_1451() {
        // sqlx-postgres migrate.rs: `{table_name}` is a Rust format! placeholder,
        // a runtime value — not a literal table identifier. The SELECT FROM and
        // INSERT INTO both target the placeholder, so there is nothing to pair.
        let src = r##"fn migrate() {
            let a = AssertSqlSafe(format!(r#"SELECT version FROM {table_name} WHERE success = false ORDER BY version LIMIT 1"#));
            let b = AssertSqlSafe(format!(r#"INSERT INTO {table_name} ( version, description ) VALUES ($1, $2)"#));
        }"##;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_double_report_same_insert_issue_1451() {
        // Two SELECT FROM statements on the same literal table both pair with the
        // first INSERT; the shared INSERT location must be reported only once.
        let src = r#"fn f() {
            let a = "SELECT version FROM users WHERE success = false";
            let b = "SELECT version, checksum FROM users ORDER BY version";
            let c = "INSERT INTO users (version) VALUES ($1)";
        }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn an_insert_select_is_one_atomic_statement() {
        let src = r#"fn copy(db: &Db) {
            db.execute("INSERT INTO pg_depend SELECT * FROM pg_depend WHERE false;");
            db.execute("INSERT INTO pg_depend SELECT * FROM pg_depend WHERE false;");
        }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn literals_of_two_assertion_calls_never_run_in_sequence() {
        let src = r#"#[test]
        fn denied() {
            assert_denied("SELECT role_name FROM audit.t;");
            assert_denied("INSERT INTO audit.t (role_name) VALUES ('x');");
        }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn two_tables_of_one_schema_are_not_one_table() {
        let src = r#"fn probe(db: &Db) {
            db.query("SELECT role_name FROM information_schema.enabled_roles;");
            db.execute("INSERT INTO information_schema.sql_features (feature_id) VALUES ('x');");
        }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn a_select_and_an_insert_in_two_functions_never_run_in_sequence() {
        let src = r#"
            fn read(db: &Db) { db.query("SELECT id FROM users WHERE email = $1"); }
            fn write(db: &Db) { db.execute("INSERT INTO users (email) VALUES ($1)"); }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_a_check_then_insert_on_one_qualified_table() {
        let src = r#"fn upsert(db: &Db) {
            let row = db.query("SELECT id FROM app.users WHERE email = $1");
            if row.is_none() {
                db.execute("INSERT INTO \"app\".\"users\" (email) VALUES ($1)");
            }
        }"#;
        let found = run(src);
        assert_eq!(found.len(), 1);
        assert!(
            found[0].message.contains("`APP.USERS`"),
            "{}",
            found[0].message
        );
    }
}
