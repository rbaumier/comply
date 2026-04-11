use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect octal escapes like `\0`, `\1`..`\7`, `\00`..`\377` inside regex
/// literals. These are ambiguous: they could mean a backreference or an
/// octal character code.
fn has_octal_escape_in_regex(line: &str) -> bool {
    // Only check lines that look like they contain a regex literal or RegExp.
    if !line.contains('/') && !line.contains("RegExp") && !line.contains("Regex::") {
        return false;
    }
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len.saturating_sub(1) {
        if bytes[i] == b'\\' {
            // Count consecutive backslashes.
            let backslashes = {
                let mut c = 0;
                let mut j = i;
                while j < len && bytes[j] == b'\\' {
                    c += 1;
                    j += 1;
                }
                c
            };
            if backslashes % 2 == 1 {
                // Odd number of backslashes — the last one is an escape.
                let esc_pos = i + backslashes - 1;
                let after = esc_pos + 1;
                if after < len && bytes[after].is_ascii_digit() && bytes[after] != b'8' && bytes[after] != b'9' {
                    // `\0` alone (null) is common and unambiguous — skip it
                    // unless followed by more octal digits.
                    if bytes[after] == b'0' {
                        if after + 1 < len && bytes[after + 1] >= b'0' && bytes[after + 1] <= b'7' {
                            return true;
                        }
                        // Bare `\0` is fine.
                    } else {
                        return true;
                    }
                }
            }
            i += backslashes;
        } else {
            i += 1;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_octal_escape_in_regex(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-octal".into(),
                    message: "Octal escape in regex is ambiguous — use a named backreference or Unicode escape instead.".into(),
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
    fn flags_octal_escape_in_regex() {
        assert_eq!(run(r#"const re = /\1/;"#).len(), 1);
    }

    #[test]
    fn flags_multi_digit_octal() {
        assert_eq!(run(r#"const re = /\12/;"#).len(), 1);
    }

    #[test]
    fn allows_null_escape() {
        assert!(run(r#"const re = /\0/;"#).is_empty());
    }

    #[test]
    fn flags_octal_after_null() {
        assert_eq!(run(r#"const re = /\00/;"#).len(), 1);
    }

    #[test]
    fn allows_no_regex() {
        assert!(run("const x = 42;").is_empty());
    }
}
