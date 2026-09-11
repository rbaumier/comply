use crate::diagnostic::Diagnostic;
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::comment_blocks;

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["--", "/*"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        super::banner_diagnostics(comment_blocks::from_line_oriented_text(ctx.source), ctx)
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
    fn flags_a_section_banner() {
        let src = "\
-- ─── Seed data ───────────────────────────
INSERT INTO plan (name) VALUES ('free');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn a_plain_comment_is_not_a_banner() {
        let src = "\
-- Seed the default plan.
INSERT INTO plan (name) VALUES ('free');";
        assert!(run(src).is_empty());
    }
}
