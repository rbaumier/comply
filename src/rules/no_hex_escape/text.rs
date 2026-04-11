use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

fn likely_in_comment(line: &str, match_start: usize) -> bool {
    let prefix = &line[..match_start];
    if let Some(pos) = prefix.find("//") {
        let before_comment = &prefix[..pos];
        let dq = before_comment.matches('"').count();
        let sq = before_comment.matches('\'').count();
        if dq.is_multiple_of(2) && sq.is_multiple_of(2) {
            return true;
        }
    }
    false
}

/// Find all `\xNN` hex escapes in a line, returning byte offset and the hex digits.
/// Skips escaped backslashes (`\\x41` is not a hex escape).
fn find_hex_escapes(line: &str) -> Vec<(usize, &str)> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut results = Vec::new();
    let mut i = 0;

    while i + 3 < len {
        if bytes[i] == b'\\' {
            // Count consecutive backslashes
            let bs_start = i;
            while i < len && bytes[i] == b'\\' {
                i += 1;
            }
            let bs_count = i - bs_start;

            // If odd number of backslashes and next char is 'x' + 2 hex digits
            if bs_count % 2 == 1
                && i < len
                && bytes[i] == b'x'
                && i + 2 < len
                && bytes[i + 1].is_ascii_hexdigit()
                && bytes[i + 2].is_ascii_hexdigit()
            {
                // The \x starts at i-1 (the last unpaired backslash)
                let hex_str = &line[i + 1..i + 3];
                results.push((i - 1, hex_str));
                i += 3; // skip past xNN
            }
            // If even number, all backslashes are escaped, continue
        } else {
            i += 1;
        }
    }

    results
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }

            for (col, hex) in find_hex_escapes(line) {
                if likely_in_comment(line, col) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "no-hex-escape".into(),
                    message: format!(
                        "Use Unicode escape `\\u00{}` instead of hex escape `\\x{}`.",
                        hex, hex
                    ),
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
    fn flags_hex_escape_in_string() {
        let d = run(r#"const x = '\x41';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("\\u0041"));
    }

    #[test]
    fn flags_hex_escape_in_double_quotes() {
        let d = run(r#"const x = "\x42";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("\\u0042"));
    }

    #[test]
    fn flags_hex_escape_in_template() {
        let d = run(r#"const x = `\x43`;"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_multiple_hex_escapes() {
        let d = run(r#"const x = "\x41\x42\x43";"#);
        assert_eq!(d.len(), 3);
    }

    #[test]
    fn allows_unicode_escape() {
        assert!(run(r#"const x = '\u0041';"#).is_empty());
    }

    #[test]
    fn allows_escaped_backslash_before_x() {
        assert!(run(r#"const x = '\\x41';"#).is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run(r#"// \x41"#).is_empty());
    }

    #[test]
    fn allows_normal_string() {
        assert!(run(r#"const x = "hello";"#).is_empty());
    }
}
