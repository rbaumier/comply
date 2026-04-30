//! comment-prose-quality backend — flag weak prose in comments:
//! weasel words, passive voice, and lexical illusions (repeated words
//! across line breaks).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const WEASEL_WORDS: &[&str] = &[
    "various",
    "many",
    "somewhat",
    "practically",
    "relatively",
    "extremely",
    "basically",
    "actually",
    "really",
    "literally",
    "quite",
    "fairly",
];

const PASSIVE_PATTERNS: &[&str] = &[
    "is used",
    "was called",
    "are handled",
    "were created",
    "been processed",
];

/// Extract comment text from a line. Returns `Some(text)` for `//` and
/// `/*` single-line comments, and lines that continue a block comment
/// (starting with `*`). Strips Rust doc-comment markers (`//!`, `///`).
fn comment_text(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("//") {
        // Strip Rust doc-comment markers: `//!` or `///`.
        let rest = rest.strip_prefix('!').unwrap_or(rest);
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        return Some(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("/*") {
        return Some(rest);
    }
    if let Some(rest) = trimmed.strip_prefix('*') {
        // Block comment continuation line.
        return Some(rest);
    }
    None
}

/// Check if a word boundary exists around the match (crude but sufficient).
fn contains_word(haystack: &str, needle: &str) -> bool {
    let lower = haystack.to_lowercase();
    let mut start = 0;
    while let Some(idx) = lower[start..].find(needle) {
        let abs = start + idx;
        let before_ok = abs == 0 || !lower.as_bytes()[abs - 1].is_ascii_alphanumeric();
        let after_pos = abs + needle.len();
        let after_ok =
            after_pos >= lower.len() || !lower.as_bytes()[after_pos].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut prev_last_word: Option<(String, usize)> = None;

        for (idx, line) in lines.iter().enumerate() {
            let Some(text) = comment_text(line) else {
                prev_last_word = None;
                continue;
            };
            let lower = text.to_lowercase();

            // Weasel words.
            for &weasel in WEASEL_WORDS {
                if contains_word(&lower, weasel) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: 1,
                        rule_id: "comment-prose-quality".into(),
                        message: format!("Weasel word `{weasel}` in comment — be specific."),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break; // One weasel diagnostic per line.
                }
            }

            // Passive voice.
            for &passive in PASSIVE_PATTERNS {
                if lower.contains(passive) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: idx + 1,
                        column: 1,
                        rule_id: "comment-prose-quality".into(),
                        message: format!(
                            "Passive voice `{passive}` in comment — use active voice."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
            }

            // Lexical illusion: last word of previous comment line == first
            // word of this comment line.
            let words: Vec<&str> = text.split_whitespace().collect();
            let is_rustdoc_heading_echo = prev_last_word.as_ref().is_some_and(|(_, prev_wc)| {
                *prev_wc == 2
                    && lines
                        .get(idx.wrapping_sub(1))
                        .and_then(|l| comment_text(l))
                        .is_some_and(|prev_text| {
                            let pt = prev_text.trim();
                            pt.starts_with("# ")
                        })
            });
            if let Some((ref prev, prev_wc)) = prev_last_word
                && words.len() > 1
                && prev_wc > 1
                && let Some(&first) = words.first()
                && first.chars().any(|c| c.is_alphabetic())
                && first.to_lowercase() == *prev
                && !is_rustdoc_heading_echo
            {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "comment-prose-quality".into(),
                    message: format!("Lexical illusion: `{first}` repeated across lines."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            prev_last_word = words
                .last()
                .filter(|w| w.chars().any(|c| c.is_alphabetic()))
                .map(|w| (w.to_lowercase(), words.len()));
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_weasel_word() {
        let diags = run("// This is basically a wrapper.");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("basically"));
    }

    #[test]
    fn flags_passive_voice() {
        let diags = run("// The value is used to compute the result.");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("is used"));
    }

    #[test]
    fn flags_lexical_illusion() {
        let src = "// This handles the\n// the processing step.";
        let diags = run(src);
        assert!(diags.iter().any(|d| d.message.contains("Lexical illusion")));
    }

    #[test]
    fn allows_clean_comment() {
        assert!(run("// Compute the SHA-256 hash of the input buffer.").is_empty());
    }

    #[test]
    fn allows_rust_doc_comments() {
        // `//!` markers should not trigger lexical illusion on `!`.
        let src = "//! Module doc.\n//!\n//! More details here.";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_rust_item_doc_comments() {
        // `///` markers should not trigger lexical illusion on `/`.
        let src = "/// Function doc.\n///\n/// More details.";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_punctuation_only_tokens_for_lexical_illusion() {
        let src = "// obj = {\n// },\n// },";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion"))
        );
    }

    #[test]
    fn ignores_closing_braces_for_lexical_illusion() {
        let src = "// }\n// }";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion"))
        );
    }

    #[test]
    fn ignores_jsdoc_star_for_lexical_illusion() {
        let src = " * @param foo - description\n * @returns bar";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion"))
        );
    }

    #[test]
    fn ignores_rustdoc_heading_echo() {
        let src = "/// # Panics\n/// Panics if the buffer is empty.";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion"))
        );
        let src = "/// # Returns\n/// Returns `None` if not found.";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion"))
        );
        let src = "/// # Errors\n/// Errors if the input is invalid.";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion"))
        );
    }
}
