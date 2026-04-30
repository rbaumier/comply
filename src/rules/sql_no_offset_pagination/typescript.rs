//! sql-no-offset-pagination — TS / JS / TSX backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{TS_STRING_KINDS, is_sql_string};

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
        if !is_sql_string(text) {
            return;
        }
        if !super::sql_uses_offset_pagination(text) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
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
