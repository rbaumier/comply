//! sql-no-varchar — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_ddl, RUST_STRING_KINDS};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, RUST_STRING_KINDS) {
            let Ok(text) = node.utf8_text(source_bytes) else {
                continue;
            };
            if !is_sql_ddl(text) {
                continue;
            }
            if !super::sql_uses_varchar_or_char(text) {
                continue;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
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
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(src, &Check)
    }

    #[test]
    fn flags_varchar_in_create_table() {
        let src = r#"fn f() { let m = "CREATE TABLE users (name VARCHAR(255))"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_char_in_raw_string_alter_table() {
        let src = r###"fn f() { let m = r#"ALTER TABLE users ADD COLUMN code CHAR(3)"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_test_function_with_char_suffix() {
        // The exact FP reported by the user — function name ends in
        // `_char(`, which used to look like the SQL keyword `CHAR(`.
        let src = "fn flags_negative_lookahead_same_char() { let x = 1; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_text_column() {
        let src = r#"fn f() { let m = "CREATE TABLE users (name TEXT)"; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_comment_with_pattern() {
        let src = "// CREATE TABLE users (name VARCHAR(255))\nfn f() {}";
        assert!(run(src).is_empty());
    }
}
