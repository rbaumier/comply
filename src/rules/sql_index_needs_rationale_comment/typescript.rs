//! sql-index-needs-rationale-comment — TypeScript / JavaScript / TSX backend.
//!
//! Walks every `string` and `template_string` node in the AST and scans
//! the content for unexplained `CREATE INDEX`. Detection logic lives in
//! the sibling `rust` module (`check_string_content`) to avoid divergence
//! between backends — the rule is about SQL text, which has no opinion
//! on its host language.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

use super::rust::check_string_content;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let min_rationale_words = ctx
            .config
            .threshold("sql-index-needs-rationale-comment", "min_rationale_words");
        let lookback_lines = ctx
            .config
            .threshold("sql-index-needs-rationale-comment", "lookback_lines");
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if !matches!(node.kind(), "string" | "template_string") {
                return;
            }
            let Ok(raw) = node.utf8_text(source_bytes) else {
                return;
            };
            let content = strip_ts_string_delimiters(raw);
            let start = node.start_position();
            // Skip the opening `"` / `'` / `` ` `` — 1 byte in every case.
            let delimiter_len = raw.len().saturating_sub(content.len()) / 2;
            diagnostics.extend(check_string_content(
                content,
                start.row,
                start.column + delimiter_len,
                ctx.path,
                min_rationale_words,
                lookback_lines,
            ));
        });
        diagnostics
    }
}

/// Strip the surrounding quote of a TS/JS string or template literal.
/// Returns the original input unchanged if the shape doesn't match — the
/// caller handles empty content gracefully.
fn strip_ts_string_delimiters(raw: &str) -> &str {
    let first = raw.chars().next();
    let last = raw.chars().last();
    match (first, last) {
        (Some('"'), Some('"')) | (Some('\''), Some('\'')) | (Some('`'), Some('`'))
            if raw.len() >= 2 =>
        {
            &raw[1..raw.len() - 1]
        }
        _ => raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_create_index_without_comment() {
        let src = "const sql = `CREATE INDEX idx_foo ON bar(baz);`;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_create_index_with_preceding_comment() {
        let src = "const sql = `-- Accelerates dashboard query for user_id\n\
                   CREATE INDEX idx_foo ON bar(baz);`;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_index_sql() {
        let src = "const sql = `SELECT * FROM foo`;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_create_unique_index() {
        let src = "const sql = `CREATE UNIQUE INDEX idx_foo ON bar(baz);`;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_trailing_comment() {
        let src = "const sql = `CREATE INDEX idx_foo ON bar(baz); -- accelerates lookups by id`;";
        assert!(run_on(src).is_empty());
    }
}
