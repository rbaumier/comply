use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Return true if the character before `pos` could be the end of an expression
/// (identifier char, `)`, `]`, or a quote) — i.e. something you'd call a method on.
fn is_callee_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b')' || b == b']'
        || b == b'\'' || b == b'"' || b == b'`'
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let bytes = line.as_bytes();
            for pattern in &[".substring(", ".substr("] {
                let pat_bytes = pattern.as_bytes();
                let mut start = 0;
                while start + pat_bytes.len() <= bytes.len() {
                    if let Some(rel) = line[start..].find(pattern) {
                        let abs = start + rel;
                        // Ensure there's a callee before the dot
                        if abs > 0 && is_callee_char(bytes[abs - 1]) {
                            let method = if *pattern == ".substring(" {
                                "substring"
                            } else {
                                "substr"
                            };
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: idx + 1,
                                column: abs + 2, // point at method name
                                rule_id: "prefer-string-slice".into(),
                                message: format!(
                                    "Prefer `String#slice()` over `String#{}()`.",
                                    method
                                ),
                                severity: Severity::Warning,
                            });
                            break; // one per line per pattern
                        }
                        start = abs + pat_bytes.len();
                    } else {
                        break;
                    }
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
    fn flags_substring() {
        let d = run("str.substring(1, 3)");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("substring"));
    }

    #[test]
    fn flags_substr() {
        let d = run("str.substr(0, 5)");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("substr"));
    }

    #[test]
    fn allows_slice() {
        assert!(run("str.slice(1, 3)").is_empty());
    }

    #[test]
    fn flags_chained_call() {
        let d = run("foo().substring(0)");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn ignores_standalone_word() {
        // Not a method call — no callee before the dot
        assert!(run("substring(1, 3)").is_empty());
    }
}
