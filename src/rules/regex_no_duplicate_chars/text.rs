use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract content of character classes `[...]` from a line and check for duplicate single chars.
fn has_duplicate_in_char_class(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            // Check it's not escaped
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 != 0 {
                i += 1;
                continue;
            }
            // Find matching ]
            let start = i + 1;
            // Skip leading ^ for negated classes
            let content_start = if start < bytes.len() && bytes[start] == b'^' {
                start + 1
            } else {
                start
            };
            let mut j = start;
            // Allow ] as first char in class
            if j < bytes.len() && bytes[j] == b']' {
                j += 1;
            }
            while j < bytes.len() && bytes[j] != b']' {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                j += 1;
            }
            if j < bytes.len() {
                let content = &line[content_start..j];
                // Collect single characters (skip escape sequences and ranges)
                let mut chars: Vec<char> = Vec::new();
                let mut ci = 0;
                let cbytes = content.as_bytes();
                while ci < cbytes.len() {
                    if cbytes[ci] == b'\\' {
                        ci += 2; // skip escape sequences
                        continue;
                    }
                    if ci + 1 < cbytes.len() && cbytes[ci + 1] == b'-' {
                        ci += 3; // skip range like a-z
                        continue;
                    }
                    chars.push(cbytes[ci] as char);
                    ci += 1;
                }
                // Check for duplicates
                let len_before = chars.len();
                chars.sort();
                chars.dedup();
                if chars.len() < len_before {
                    return true;
                }
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_duplicate_in_char_class(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-duplicate-chars".into(),
                    message: "Duplicate character in regex character class — remove the redundant character.".into(),
                    severity: Severity::Warning,
                    span: None,
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
    fn flags_duplicate_chars() {
        assert_eq!(run("const re = /[aab]/;").len(), 1);
    }

    #[test]
    fn flags_duplicate_chars_non_adjacent() {
        assert_eq!(run("const re = /[aba]/;").len(), 1);
    }

    #[test]
    fn allows_unique_chars() {
        assert!(run("const re = /[abc]/;").is_empty());
    }

    #[test]
    fn allows_ranges() {
        assert!(run("const re = /[a-z]/;").is_empty());
    }
}
