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
        let pos = node.start_position();
        let range = node.byte_range();
        state.nodes.push(Collected {
            text: text.to_string(),
            line: pos.row + 1,
            column: pos.column + 1,
            byte_start: range.start,
            byte_len: range.len(),
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
                    severity: Severity::Warning,
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
}
