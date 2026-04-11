use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Return true if the byte could be the last char of a callee expression.
fn is_callee_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b')' || b == b']'
        || b == b'\'' || b == b'"'
}

/// Check for `.trimLeft(` or `.trimRight(` as a method call (callee before the dot).
fn has_method_call(line: &str, pattern: &str) -> bool {
    let bytes = line.as_bytes();
    let mut start = 0;
    while start + pattern.len() <= bytes.len() {
        let Some(rel) = line[start..].find(pattern) else {
            break;
        };
        let abs = start + rel;
        if abs > 0 && is_callee_char(bytes[abs - 1]) {
            return true;
        }
        start = abs + pattern.len();
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for (pattern, replacement) in &[
                (".trimLeft(", "trimStart"),
                (".trimRight(", "trimEnd"),
            ] {
                if has_method_call(line, pattern) {
                    let method = &pattern[1..pattern.len() - 1]; // strip leading `.` and trailing `(`
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "prefer-string-trim-start-end".into(),
                        message: format!(
                            "Prefer `String#{}()` over `String#{}()`.",
                            replacement, method
                        ),
                        severity: Severity::Warning,
                    });
                    break; // one diagnostic per line
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }

    #[test]
    fn flags_trim_left() {
        let d = run("str.trimLeft()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("trimStart"));
    }

    #[test]
    fn flags_trim_right() {
        let d = run("str.trimRight()");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("trimEnd"));
    }

    #[test]
    fn allows_trim_start() {
        assert!(run("str.trimStart()").is_empty());
    }

    #[test]
    fn allows_trim_end() {
        assert!(run("str.trimEnd()").is_empty());
    }

    #[test]
    fn allows_plain_trim() {
        assert!(run("str.trim()").is_empty());
    }

    #[test]
    fn ignores_standalone_function() {
        assert!(run("trimLeft()").is_empty());
    }
}
