//! banned-comment-words backend — scan comment lines for dismissive filler.
//!
//! Each match must be (a) inside a comment (we look for the `//` or `/*`
//! marker first) and (b) on a word boundary so we don't false-positive on
//! `simplify` matching `simply` or `understanding` matching nothing. The
//! word list is closed: 6 entries, all unambiguously dismissive in English.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const BANNED: &[&str] = &[
    "obviously",
    "simply",
    "just",
    "basically",
    "clearly",
    "trivially",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let Some(comment_text) = comment_body(line) else {
                continue;
            };
            for &word in BANNED {
                if !contains_word_boundary(comment_text, word) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "banned-comment-words".into(),
                    message: format!(
                        "Comment uses `{word}` — dismissive filler that hides complexity. \
                         Either explain the actual subtlety or delete the comment if the \
                         line is genuinely self-explanatory."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                break; // one diagnostic per line is enough
            }
        }
        diagnostics
    }
}

/// Return the comment body (everything after `//` or `/*`) for this line,
/// lowercased on demand by the caller. Returns None if the line has no
/// comment marker.
fn comment_body(line: &str) -> Option<&str> {
    let pos = line.find("//").or_else(|| line.find("/*"))?;
    Some(&line[pos..])
}

/// Case-insensitive word-boundary substring match — `word` must be preceded
/// and followed by a non-letter character (or string boundary).
fn contains_word_boundary(haystack: &str, word: &str) -> bool {
    let h_lower = haystack.to_ascii_lowercase();
    let bytes = h_lower.as_bytes();
    let needle = word.as_bytes();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let prev_ok = i == 0 || !bytes[i - 1].is_ascii_alphabetic();
            let next_ok = i + needle.len() == bytes.len()
                || !bytes[i + needle.len()].is_ascii_alphabetic();
            if prev_ok && next_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_simply() {
        assert_eq!(run("// This simply works").len(), 1);
    }

    #[test]
    fn flags_obviously() {
        assert_eq!(run("// Obviously the cache wins").len(), 1);
    }

    #[test]
    fn flags_just() {
        assert_eq!(run("// just retry on failure").len(), 1);
    }

    #[test]
    fn allows_simplify() {
        // word boundary: `simplify` contains `simply` only as a prefix-ish
        // substring; word boundary check rejects it.
        assert!(run("// We simplify the input").is_empty());
    }

    #[test]
    fn allows_understanding() {
        // No banned word inside.
        assert!(run("// understanding the data flow").is_empty());
    }

    #[test]
    fn ignores_banned_word_in_code() {
        // Outside a comment, the rule must not fire.
        assert!(run("const obviously = true;").is_empty());
    }

    #[test]
    fn one_diagnostic_per_line() {
        assert_eq!(run("// just simply works").len(), 1);
    }

    #[test]
    fn flags_block_comment() {
        assert_eq!(run("/* this is basically wrong */").len(), 1);
    }
}
