//! sql-no-between-timestamp — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_string, RUST_STRING_KINDS};
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
            if !is_sql_string(text) {
                continue;
            }
            if !super::sql_uses_between_on_timestamp(text) {
                continue;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "sql-no-between-timestamp".into(),
                message: "`BETWEEN` with timestamps is inclusive on both \
                          sides — use `>= start AND < end` instead."
                    .into(),
                severity: Severity::Warning,
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
    fn flags_string_literal_sql() {
        let src = r#"fn f() { let q = "SELECT * FROM events WHERE created_at BETWEEN '2024-01-01' AND '2024-12-31'"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_raw_string_literal_sql() {
        let src = r###"fn f() { let q = r#"SELECT * FROM logs WHERE event_at BETWEEN $1 AND $2"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_comment_with_pattern() {
        let src = "// WHERE created_at BETWEEN start AND end\nfn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_identifier_named_between() {
        let src = "fn f() { let between_timestamps = true; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_between_on_id() {
        let src = r#"fn f() { let q = "SELECT * FROM users WHERE id BETWEEN 1 AND 100"; }"#;
        assert!(run(src).is_empty());
    }
}
