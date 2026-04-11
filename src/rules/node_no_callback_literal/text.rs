use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if the line contains `cb('...` / `callback("...` / `next('...` —
/// a callback invoked with a string literal as its first argument.
fn has_callback_literal(line: &str) -> bool {
    for name in &["cb", "callback", "next"] {
        let mut start = 0;
        while let Some(pos) = line[start..].find(name) {
            let abs = start + pos;
            // Make sure it's a standalone identifier.
            if abs > 0 {
                let prev = line.as_bytes()[abs - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    start = abs + name.len();
                    continue;
                }
            }
            let after = abs + name.len();
            if let Some(rest) = line.get(after..) {
                let trimmed = rest.trim_start();
                // Check for `(` followed by a string literal.
                if trimmed.starts_with("('") || trimmed.starts_with("(\"") || trimmed.starts_with("(`") {
                    return true;
                }
            }
            start = abs + name.len();
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
            if has_callback_literal(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "node-no-callback-literal".into(),
                    message: "Unexpected string literal in error position of callback. Pass `new Error(...)` or `null` instead.".into(),
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
    fn flags_cb_with_single_quote_string() {
        assert_eq!(run("cb('something went wrong');").len(), 1);
    }

    #[test]
    fn flags_callback_with_double_quote_string() {
        assert_eq!(run(r#"callback("error occurred");"#).len(), 1);
    }

    #[test]
    fn flags_next_with_string() {
        assert_eq!(run("next('fail');").len(), 1);
    }

    #[test]
    fn allows_cb_with_error_object() {
        assert!(run("cb(new Error('oops'));").is_empty());
    }

    #[test]
    fn allows_cb_with_null() {
        assert!(run("cb(null, data);").is_empty());
    }

    #[test]
    fn allows_cb_with_variable() {
        assert!(run("cb(err);").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// cb('error')").is_empty());
    }

    #[test]
    fn does_not_flag_substring() {
        // `mycb` should not match — `cb` must be a standalone identifier.
        assert!(run("mycb('test');").is_empty());
    }
}
