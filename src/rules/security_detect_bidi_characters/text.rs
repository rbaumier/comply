//! security-detect-bidi-characters text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Trojan-source-relevant bidi control codepoints.
const BIDI_CHARS: &[char] = &[
    '\u{202A}', // LRE — Left-to-Right Embedding
    '\u{202B}', // RLE — Right-to-Left Embedding
    '\u{202C}', // PDF — Pop Directional Formatting
    '\u{202D}', // LRO — Left-to-Right Override
    '\u{202E}', // RLO — Right-to-Left Override
    '\u{2066}', // LRI — Left-to-Right Isolate
    '\u{2067}', // RLI — Right-to-Left Isolate
    '\u{2068}', // FSI — First Strong Isolate
    '\u{2069}', // PDI — Pop Directional Isolate
];

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Skip the per-char UTF-8 decode on files containing none of the
        // bidi control characters (the overwhelming majority).
        Some(&[
            "\u{202A}", "\u{202B}", "\u{202C}", "\u{202D}", "\u{202E}", "\u{2066}", "\u{2067}",
            "\u{2068}", "\u{2069}",
        ])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (line_idx, line) in ctx.source.lines().enumerate() {
            for (col_idx, ch) in line.chars().enumerate() {
                if BIDI_CHARS.contains(&ch) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: line_idx + 1,
                        column: col_idx + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Unicode bidi control character U+{:04X} — trojan-source attack vector. Remove it.",
                            ch as u32
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_rlo_in_comment() {
        let src = "// access_level = \"user\u{202E} // admin\"";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pure_ascii_source() {
        let src = "const x = 1;\n// normal comment\n";
        assert!(run(src).is_empty());
    }
}
