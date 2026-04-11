//! escape-case text backend — flag lowercase hex digits in escape sequences.
//!
//! Detects patterns like `\xff`, `\u00ff`, `\u{ff}` and flags them when the
//! hex digits contain lowercase letters. The fix is to uppercase them:
//! `\xFF`, `\u00FF`, `\u{FF}`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

/// Matches escape sequences with hex digits: \xNN, \uNNNN, \u{N+}
/// Ensures there's an odd number of preceding backslashes (i.e. the backslash
/// is not itself escaped).
static RE_ESCAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\(x[0-9A-Fa-f]{2}|u[0-9A-Fa-f]{4}|u\{[0-9A-Fa-f]+\})").unwrap());

/// Returns true if the escape at `pos` is preceded by an even number of
/// backslashes (meaning the backslash at `pos` is itself escaped).
fn is_escaped(line: &str, pos: usize) -> bool {
    let prefix = &line[..pos];
    let trailing_backslashes = prefix.len() - prefix.trim_end_matches('\\').len();
    !trailing_backslashes.is_multiple_of(2)
}

/// Returns true if the match position is likely inside a comment.
fn in_comment(line: &str, pos: usize) -> bool {
    let prefix = &line[..pos];
    prefix.contains("//")
}

/// Check if the hex portion of an escape sequence contains any lowercase letters.
fn has_lowercase_hex(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_ascii_lowercase() && c.is_ascii_hexdigit())
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            // Skip full-line comments.
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }

            for mat in RE_ESCAPE.find_iter(line) {
                let start = mat.start();

                // Skip if the backslash is itself escaped.
                if is_escaped(line, start) {
                    continue;
                }

                // Skip if inside a trailing comment.
                if in_comment(line, start) {
                    continue;
                }

                let matched = mat.as_str();
                // The escape body is everything after the leading `\`.
                let body = &matched[1..];

                if !has_lowercase_hex(body) {
                    continue;
                }

                let uppercased = format!("\\{}", uppercase_hex(body));

                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: start + 1,
                    rule_id: "escape-case".into(),
                    message: format!(
                        "Use uppercase characters for the value of the escape sequence: \
                         `{matched}` -> `{uppercased}`."
                    ),
                    severity: Severity::Warning,
                });
            }
        }

        diagnostics
    }
}

/// Uppercase only the hex digits in an escape body (preserving the prefix letter).
/// E.g. `xff` -> `xFF`, `u00ff` -> `u00FF`, `u{ff}` -> `u{FF}`.
fn uppercase_hex(body: &str) -> String {
    body.chars()
        .map(|c| {
            if c.is_ascii_hexdigit() && c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_lowercase_hex_escape() {
        let d = run(r#"const a = "\xff";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\xFF"));
    }

    #[test]
    fn flags_lowercase_unicode_escape() {
        let d = run(r#"const a = "\u00ff";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\u00FF"));
    }

    #[test]
    fn flags_lowercase_unicode_brace_escape() {
        let d = run(r#"const a = "\u{1a2b}";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\u{1A2B}"));
    }

    #[test]
    fn allows_uppercase_escape() {
        assert!(run(r#"const a = "\xFF";"#).is_empty());
    }

    #[test]
    fn allows_uppercase_unicode() {
        assert!(run(r#"const a = "\u00FF";"#).is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run(r#"// const a = "\xff";"#).is_empty());
    }

    #[test]
    fn flags_multiple_on_one_line() {
        let d = run(r#"const a = "\xff\u00ab";"#);
        assert_eq!(d.len(), 2);
    }
}
