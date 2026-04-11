use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects patterns like `(?=a)b` or `(?!a)a` where an assertion contradicts
/// the element that follows/precedes it, making the branch unmatchable.
fn find_contradiction(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 5 < len {
        // Lookahead followed by contradictory char: (?=X)Y where X != Y
        if bytes[i] == b'(' && i + 4 < len && bytes[i + 1] == b'?' && bytes[i + 2] == b'=' {
            let assert_char_pos = i + 3;
            if assert_char_pos < len && bytes[assert_char_pos] != b')' && bytes[assert_char_pos] != b'\\' {
                // Find the closing paren
                if let Some(close) = find_close_paren(bytes, i) {
                    let after = close + 1;
                    if after < len
                        && bytes[after] != b'|'
                        && bytes[after] != b')'
                        && bytes[after] != b'('
                        && bytes[assert_char_pos] != bytes[after]
                        && bytes[after].is_ascii_alphanumeric()
                        && bytes[assert_char_pos].is_ascii_alphanumeric()
                    {
                        hits.push(i);
                    }
                }
            }
        }
        // Negative lookahead followed by same char: (?!X)X
        if bytes[i] == b'(' && i + 4 < len && bytes[i + 1] == b'?' && bytes[i + 2] == b'!' {
            let assert_char_pos = i + 3;
            if assert_char_pos < len && bytes[assert_char_pos] != b')' && bytes[assert_char_pos] != b'\\'
                && let Some(close) = find_close_paren(bytes, i) {
                    let after = close + 1;
                    if after < len && bytes[assert_char_pos] == bytes[after] {
                        hits.push(i);
                    }
                }
        }
        i += 1;
    }
    hits
}

fn find_close_paren(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1;
    let mut j = start + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_contradiction(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-contradiction-with-assertion".into(),
                    message: "Assertion contradicts the pattern around it \u{2014} this branch can never match.".into(),
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
    fn flags_positive_lookahead_contradiction() {
        assert_eq!(run(r#"const re = /(?=a)b/;"#).len(), 1);
    }

    #[test]
    fn flags_negative_lookahead_same_char() {
        assert_eq!(run(r#"const re = /(?!a)a/;"#).len(), 1);
    }

    #[test]
    fn allows_consistent_lookahead() {
        assert!(run(r#"const re = /(?=a)a/;"#).is_empty());
    }
}
