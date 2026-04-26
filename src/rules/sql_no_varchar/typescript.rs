//! sql-no-varchar — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_ddl, TS_STRING_KINDS};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(TS_STRING_KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        if !is_sql_ddl(text) {
            return;
        }
        if !super::sql_uses_varchar_or_char(text) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "sql-no-varchar".into(),
            message: "`VARCHAR(N)` / `CHAR(N)` provides no perf benefit \
                      in PostgreSQL — use `TEXT` with \
                      `CHECK(length(col) <= N)`."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_varchar_in_create_table() {
        let src = r#"const m = "CREATE TABLE users (name VARCHAR(255))";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_char_in_alter_table() {
        let src = r#"const m = "ALTER TABLE users ADD COLUMN code CHAR(3)";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_text_column() {
        let src = r#"const m = "CREATE TABLE users (name TEXT)";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_test_function_with_char_suffix() {
        // The user's reported FP — function name has `_char(`, not a SQL keyword.
        let src = r"function flags_negative_lookahead_same_char() { return 1; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_dml_query() {
        // SELECT containing `varchar` as a CAST target — but it's DML, so
        // is_sql_ddl rejects the string before VARCHAR detection runs.
        let src = r#"const q = "SELECT CAST(x AS VARCHAR(255)) FROM t WHERE id = 1";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_comment_with_pattern() {
        let src = "// CREATE TABLE users (name VARCHAR(255))\nconst x = 1;";
        assert!(run(src).is_empty());
    }
}
