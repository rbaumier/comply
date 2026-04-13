//! sql-no-offset-pagination — Rust backend.

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
            if !super::sql_uses_offset_pagination(text) {
                continue;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "sql-no-offset-pagination".into(),
                message: "`OFFSET` pagination is O(N) on deep pages — use \
                          cursor-based pagination: \
                          `WHERE id > :last_id ORDER BY id LIMIT N`."
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
    fn flags_string_literal_pagination() {
        let src = r#"fn f() { let q = "SELECT * FROM users LIMIT 10 OFFSET 100"; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_raw_string_literal_pagination() {
        let src = r###"fn f() { let q = r#"SELECT * FROM users LIMIT $1 OFFSET $2"#; }"###;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_string_array_with_keyword_words() {
        // The exact FP family from the user's report: an identifier
        // list with `"offset"` and `"limit"` as plain string literals.
        let src = r#"fn f() { let bases = &["delay", "offset", "width", "limit", "rate"]; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_comment_with_pattern() {
        let src = "// SELECT ... LIMIT 10 OFFSET 100\nfn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_sql_without_offset() {
        let src = r#"fn f() { let q = "SELECT * FROM users LIMIT 10"; }"#;
        assert!(run(src).is_empty());
    }
}
