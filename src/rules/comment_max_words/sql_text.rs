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

        super::flagged_sentences(comments, max)
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
    fn flags_a_sentence_wrapped_over_several_lines() {
        let src = "\
-- Holds the connection settings and builds one client per call because the
-- underlying client opens a fresh connection per request anyway and only needs
-- a mutable handle to stash the response headers it just read.
CREATE TABLE endpoint (id uuid PRIMARY KEY);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_sentences() {
        let src = "\
-- Holds the connection settings.
-- Each call builds its own client.
CREATE TABLE endpoint (id uuid PRIMARY KEY);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_a_long_block_comment() {
        let src = "\
/* Holds the connection settings and builds one client per call because the
   underlying client opens a fresh connection per request anyway and only needs
   a mutable handle to stash the response headers it just read. */
CREATE TABLE endpoint (id uuid PRIMARY KEY);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn a_marker_inside_a_statement_opens_nothing() {
        let src = "INSERT INTO note (body) VALUES ('one two three four five six seven eight nine ten -- eleven twelve');";
        assert!(run(src).is_empty());
    }
}
