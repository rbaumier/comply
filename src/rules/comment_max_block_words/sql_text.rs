use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::comment_blocks;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["--", "/*"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let max = ctx.config.threshold(super::META.id, "max", ctx.lang);
        let comments = comment_blocks::from_line_oriented_text(ctx.source);

        super::flagged_blocks(comments, ctx.source, max)
            .into_iter()
            .map(|flag| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: flag.line,
                column: flag.column,
                rule_id: super::META.id.into(),
                message: super::message(flag.words, max),
                severity: Severity::Error,
                span: None,
            })
            .collect()
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
    fn flags_a_long_comment_block() {
        let src = "\
-- This documents the migration in full prose across several lines and words,
-- legitimately explaining the contract, invariants, and edge cases at length,
-- which is exactly what a leading comment is for and still budgeted here,
-- because a comment nobody reads to the end documents nothing at all today.
-- The budget applies to every comment the reader has to walk through in order.
CREATE TABLE endpoint (id uuid PRIMARY KEY);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn paragraphs_of_one_header_share_the_budget() {
        let src = "\
-- Deployed copy of analysis.get_analyses_per_hour (prod).
-- No migration pipeline; this file is the reference.

-- Shares its filter block with the three sibling RPCs.
-- Keep the four files in step.

-- Hours are Paris wall time, on purpose.
-- The customer sizes French teams.
BEGIN;";
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 1);
    }

    #[test]
    fn allows_a_short_block() {
        let src = "-- Holds the endpoint.\n-- One row per tenant.\nCREATE TABLE endpoint (id uuid PRIMARY KEY);";
        assert!(run(src).is_empty());
    }
}
