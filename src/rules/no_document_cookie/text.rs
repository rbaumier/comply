use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects `document.cookie` access (read or write).
fn has_document_cookie(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("document.cookie") {
        let abs = start + pos;
        // Verify not part of a longer identifier before `document`
        if abs > 0 {
            let prev = line.as_bytes()[abs - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' {
                start = abs + 15;
                continue;
            }
        }
        // Verify not part of a longer identifier after `cookie`
        let after = abs + 15; // "document.cookie".len()
        if after < line.len() {
            let next = line.as_bytes()[after];
            if next.is_ascii_alphanumeric() || next == b'_' {
                start = after;
                continue;
            }
        }
        return true;
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
            if has_document_cookie(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-document-cookie".into(),
                    message: "Do not use `document.cookie` directly — use a cookie library instead."
                        .into(),
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
    fn flags_cookie_read() {
        assert_eq!(run("const c = document.cookie;").len(), 1);
    }

    #[test]
    fn flags_cookie_write() {
        assert_eq!(run(r#"document.cookie = "a=1";"#).len(), 1);
    }

    #[test]
    fn allows_comment() {
        assert!(run("// document.cookie is bad").is_empty());
    }

    #[test]
    fn allows_unrelated_cookie() {
        assert!(run("const cookie = getCookie();").is_empty());
    }
}
