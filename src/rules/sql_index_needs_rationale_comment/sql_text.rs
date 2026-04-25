//! sql-index-needs-rationale-comment — `.sql` file backend.
//!
//! In a pure SQL file the entire content is the "string content" the
//! AST backends scan inside string literals. Reuse `check_string_content`
//! directly with `node_start_line=0` / `node_start_col=0` so the
//! diagnostic line/column point to the actual `CREATE INDEX` line.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{CheckCtx, TextCheck};

use super::rust::check_string_content;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let min_rationale_words = ctx
            .config
            .threshold("sql-index-needs-rationale-comment", "min_rationale_words");
        let lookback_lines = ctx
            .config
            .threshold("sql-index-needs-rationale-comment", "lookback_lines");
        check_string_content(
            ctx.source,
            0,
            0,
            ctx.path,
            min_rationale_words,
            lookback_lines,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), src))
    }

    #[test]
    fn flags_create_index_without_comment() {
        assert_eq!(run("CREATE INDEX idx_foo ON bar(baz);").len(), 1);
    }

    #[test]
    fn allows_create_index_with_preceding_comment() {
        let src =
            "-- Accelerates dashboard query for user_id\nCREATE INDEX idx_foo ON bar(baz);";
        assert!(run(src).is_empty());
    }
}
