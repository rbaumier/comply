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
        let comments = comment_blocks::from_line_oriented_text(ctx.source);

        super::flagged_wraps(comments)
            .into_iter()
            .map(|flag| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: flag.line,
                column: flag.column,
                rule_id: super::META.id.into(),
                message: super::MESSAGE.into(),
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
    fn flags_a_line_breaking_on_a_word() {
        let src = "\
-- Holds the connection settings of the endpoint and posts every
-- non-stream completion through the pooled client.
CREATE TABLE endpoint (id uuid PRIMARY KEY);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_a_line_broken_after_punctuation() {
        let src = "\
-- Holds the connection settings of the endpoint,
-- so every completion posts through the pooled client.
CREATE TABLE endpoint (id uuid PRIMARY KEY);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn a_marker_inside_a_statement_opens_nothing() {
        let src = "INSERT INTO note (body) VALUES ('one -- two\nthree -- four');";
        assert!(run(src).is_empty());
    }
}
