//! sql-no-offset-pagination — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::sql_helpers::{is_sql_string, RUST_STRING_KINDS};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(RUST_STRING_KINDS)
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
