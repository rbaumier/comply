use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects patterns like `a|b|c` where all alternatives are single characters.
/// Returns the byte offset of the start of the alternation if found.
fn find_single_char_alternation(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    // Look for regex-like contexts: /.../ or new RegExp("...")
    // Simple heuristic: find `X|Y` where X and Y are single non-special chars
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        // Look for single-char | single-char pattern (at least 3 alternatives: X|Y|Z)
        if i + 4 < len
            && bytes[i].is_ascii_alphanumeric()
            && bytes[i + 1] == b'|'
            && bytes[i + 2].is_ascii_alphanumeric()
            && bytes[i + 3] == b'|'
            && bytes[i + 4].is_ascii_alphanumeric()
        {
            // Verify this isn't inside a character class already
            // and that surrounding chars suggest we're not in normal code (e.g. `||`)
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            // Find the end of the alternation chain
            let mut end = i + 4;
            while end + 2 < len && bytes[end + 1] == b'|' && bytes[end + 2].is_ascii_alphanumeric() {
                end += 2;
            }
            let after_ok = end + 1 >= len || !bytes[end + 1].is_ascii_alphanumeric();
            if before_ok && after_ok {
                hits.push(i);
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    hits
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_single_char_alternation(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-prefer-char-class".into(),
                    message: "Single-character alternation \u{2014} use a character class like `[abc]` instead of `a|b|c`.".into(),
                    severity: Severity::Warning,
                });
            }
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
    fn flags_three_char_alternation() {
        let diags = run(r#"const re = /a|b|c/;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_four_char_alternation() {
        let diags = run(r#"const re = /x|y|z|w/;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_multi_char_alternatives() {
        // "foo|bar" are not single-char alternatives
        assert!(run(r#"const re = /foo|bar|baz/;"#).is_empty());
    }

    #[test]
    fn allows_two_char_alternation() {
        // Only two alternatives — not flagged (could be intentional)
        assert!(run(r#"const re = /a|b/;"#).is_empty());
    }
}
