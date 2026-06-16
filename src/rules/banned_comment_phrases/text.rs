//! banned-comment-phrases backend — scan comment lines for AI-tell preambles.
//!
//! Each match must sit inside a comment: we find the `//` or `/*` marker
//! first and scan only from there, reusing `super::find_banned_phrase` so the
//! phrase list lives in one place.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let Some(comment_text) = comment_body(line) else {
                continue;
            };
            let Some(phrase) = super::find_banned_phrase(comment_text) else {
                continue;
            };
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Comment uses `{phrase}` — narrator filler typical of AI-generated \
                     prose. State the point directly or delete the comment."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

/// Return the comment body (everything after `//` or `/*`) for this line.
/// Returns None if the line has no comment marker.
fn comment_body(line: &str) -> Option<&str> {
    let pos = line.find("//").or_else(|| line.find("/*"))?;
    Some(&line[pos..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source))
    }

    #[test]
    fn flags_walk_you_through() {
        assert_eq!(run("// let me walk you through the watcher").len(), 1);
    }

    #[test]
    fn allows_clean_comment() {
        assert!(run("// debounced to 200ms to coalesce keystrokes").is_empty());
    }

    #[test]
    fn ignores_phrase_outside_comment() {
        assert!(run("const x = onSamePage();").is_empty());
    }
}
