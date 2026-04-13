//! sql-no-offset-pagination — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_string, TS_STRING_KINDS};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, TS_STRING_KINDS) {
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
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_template_literal_pagination() {
        let src = r"const q = `SELECT * FROM users LIMIT 10 OFFSET 100`;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_string_literal_pagination_with_placeholders() {
        let src = r#"const q = "SELECT * FROM users LIMIT ? OFFSET ?";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_string_array_with_keyword_words() {
        // The user's reported FP family — a Rust-style identifier list
        // ported to TS: each "offset"/"limit" is its OWN string literal,
        // not a SQL query.
        let src = r#"const bases = ["delay", "offset", "width", "limit", "rate"];"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_comment_with_pattern() {
        let src = "// SELECT ... LIMIT 10 OFFSET 100\nconst x = 1;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_sql_without_offset() {
        let src = r#"const q = "SELECT * FROM users LIMIT 10";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_non_sql_string_with_keywords() {
        let src = r#"const x = "the limit is the offset of the field";"#;
        assert!(run(src).is_empty());
    }
}
