use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Characters that are valid after a backslash in regex.
/// Standard escapes: d, D, w, W, s, S, b, B, n, r, t, f, v, 0,
/// plus anchors / grouping: k, p, P, u, x, c
/// plus regex metacharacters that need escaping: . * + ? ^ $ { } [ ] ( ) | / \
const VALID_AFTER_BACKSLASH: &[u8] = b"dDwWsSnrtfvbB0kpPuxc.*+?^${}[]()|\\/123456789";

fn has_standalone_backslash(line: &str) -> bool {
    if !line.contains('/') && !line.contains("RegExp") && !line.contains("Regex::") {
        return false;
    }
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len.saturating_sub(1) {
        if bytes[i] == b'\\' {
            let next = bytes[i + 1];
            if next == b'\\' {
                // Escaped backslash — skip both.
                i += 2;
                continue;
            }
            if !VALID_AFTER_BACKSLASH.contains(&next) && next.is_ascii_alphabetic() {
                return true;
            }
            i += 2;
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
            if has_standalone_backslash(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-standalone-backslash".into(),
                    message: "Backslash followed by non-special character is an identity escape — likely a mistake.".into(),
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
    fn flags_backslash_before_normal_letter() {
        // \a is not a valid regex escape
        assert_eq!(run(r#"const re = /\a/;"#).len(), 1);
    }

    #[test]
    fn flags_backslash_e() {
        assert_eq!(run(r#"const re = /\e/;"#).len(), 1);
    }

    #[test]
    fn allows_valid_escape_d() {
        assert!(run(r#"const re = /\d+/;"#).is_empty());
    }

    #[test]
    fn allows_valid_escape_w() {
        assert!(run(r#"const re = /\w+/;"#).is_empty());
    }

    #[test]
    fn allows_escaped_dot() {
        assert!(run(r#"const re = /\./;"#).is_empty());
    }
}
