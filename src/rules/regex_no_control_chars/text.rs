use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect control character escapes `\x00`-`\x1f` in regex patterns.
fn has_control_chars(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'x' {
            // Parse the two hex digits
            let h1 = bytes.get(i + 2).copied();
            let h2 = bytes.get(i + 3).copied();
            if let (Some(d1), Some(d2)) = (h1, h2)
                && d1.is_ascii_hexdigit() && d2.is_ascii_hexdigit() {
                    let val = hex_val(d1) * 16 + hex_val(d2);
                    if val <= 0x1f {
                        return true;
                    }
                }
            // Also handle single-digit like \x0 (less common but mentioned in spec)
            if let Some(d1) = h1
                && d1.is_ascii_hexdigit()
                    && let Some(d2) = h2
                        && !d2.is_ascii_hexdigit() {
                            // Single hex digit: \x0 through \xf
                            let val = hex_val(d1);
                            if val <= 0x1f {
                                return true;
                            }
                        }
        }
        i += 1;
    }
    false
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_control_chars(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-control-chars".into(),
                    message: "Control character escape (`\\x00`-`\\x1f`) in regex — likely unintended.".into(),
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
    fn flags_null_byte() {
        assert_eq!(run(r#"const re = /\x00/;"#).len(), 1);
    }

    #[test]
    fn flags_control_char_1f() {
        assert_eq!(run(r#"const re = /\x1f/;"#).len(), 1);
    }

    #[test]
    fn allows_printable_hex() {
        assert!(run(r#"const re = /\x20/;"#).is_empty());
    }

    #[test]
    fn allows_upper_hex() {
        assert!(run(r#"const re = /\xFF/;"#).is_empty());
    }
}
