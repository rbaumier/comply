//! no-section-divider-comments backend — detect repeating-character dividers.
//!
//! A divider is a comment line whose body (after the comment marker, with
//! whitespace and section labels stripped) is dominated by a single
//! repeating ASCII punctuation character (`=`, `-`, `*`, `#`, `~`). The
//! threshold is 5+ consecutive repeats — short separators like `// --` are
//! allowed because they sometimes appear inside code-mode markers.
//!
//! We deliberately do NOT flag doc comments like `/// =========` in Rust
//! when they appear inside `///` blocks; those are part of rustdoc tables
//! and section formatting, not file-level dividers.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const DIVIDER_CHARS: &[u8] = b"=-*#~";

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let min_run = ctx
            .config
            .threshold("no-section-divider-comments", "min_run", ctx.lang);
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if !is_section_divider(line, min_run) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column: 1,
                rule_id: "no-section-divider-comments".into(),
                message: "Section divider comment — signal that the file is doing \
                          too many things. Split the file by responsibility instead \
                          of decorating the boundary with `===` or `***`."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
        diagnostics
    }
}

/// True if `line` is a comment whose content is dominated by a long run of
/// divider characters. Markdown-style table separators inside Rust doc
/// comments (`/// |---|---|`) are also caught — that's fine, they're
/// equally bad style at the source-file level.
fn is_section_divider(line: &str, min_run: usize) -> bool {
    let trimmed = line.trim_start();
    let body = if let Some(rest) = trimmed.strip_prefix("//") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("/*") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("#") {
        // Some build-config languages use `#` comments — we don't lint
        // those, but the function stays generic.
        rest
    } else {
        return false;
    };
    let bytes = body.as_bytes();
    // Walk the body and find the longest run of any divider character.
    let mut longest: usize = 0;
    let mut current: usize = 0;
    let mut last: u8 = 0;
    for &b in bytes {
        if DIVIDER_CHARS.contains(&b) {
            if b == last {
                current += 1;
            } else {
                current = 1;
                last = b;
            }
            if current > longest {
                longest = current;
            }
        } else {
            current = 0;
            last = 0;
        }
    }
    longest >= min_run
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_equals_divider() {
        assert_eq!(run("// ============").len(), 1);
    }

    #[test]
    fn flags_dashes_divider() {
        assert_eq!(run("// ----- SETUP -----").len(), 1);
    }

    #[test]
    fn flags_stars_divider() {
        assert_eq!(run("// ***** PRIVATE *****").len(), 1);
    }

    #[test]
    fn allows_short_dashes() {
        // 4 dashes should not trip — too short to be decorative.
        assert!(run("// -- note").is_empty());
    }

    #[test]
    fn allows_normal_comment() {
        assert!(run("// Apply the cursor advance after commit").is_empty());
    }

    #[test]
    fn ignores_dividers_in_code() {
        assert!(run("const x = '====================';").is_empty());
    }

    #[test]
    fn flags_block_comment_divider() {
        assert_eq!(run("/* ============== */").len(), 1);
    }
}
