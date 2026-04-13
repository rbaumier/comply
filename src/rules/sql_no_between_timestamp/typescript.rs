//! sql-no-between-timestamp — TS / JS / TSX backend.

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
    fn flags_template_literal_sql() {
        let src = r"const q = `SELECT * FROM events WHERE created_at BETWEEN ${a} AND ${b}`;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_string_literal_sql() {
        let src = r#"const q = "SELECT * FROM events WHERE updated_at BETWEEN '2024-01-01' AND '2024-12-31'";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_comment_with_pattern() {
        // The user's reported FP family — a comment mentioning the
        // pattern must not trigger the rule.
        let src = "// WHERE created_at BETWEEN start AND end\nconst x = 1;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_identifier_named_between_timestamps() {
        let src = "const between_timestamps = true;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_non_sql_string_with_keywords() {
        let src = r#"const x = "the user selected items delivered from the store between two timestamps";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_between_on_id_column() {
        let src = r#"const q = "SELECT * FROM users WHERE id BETWEEN 1 AND 100";"#;
        assert!(run(src).is_empty());
    }
}
