use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const DEPRECATED_PROPS: &[&str] = &["keyCode", "charCode", "which"];

/// Detects access to `event.keyCode`, `event.charCode`, or `event.which`
/// (or destructuring patterns like `{ keyCode }` from an event parameter).
fn find_deprecated_key_prop(line: &str) -> Option<&'static str> {
    for &prop in DEPRECATED_PROPS {
        let mut start = 0;
        while let Some(pos) = line[start..].find(prop) {
            let abs = start + pos;
            let after = abs + prop.len();
            // Check that prop is not part of a longer identifier
            if abs > 0 {
                let prev = line.as_bytes()[abs - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    start = after;
                    continue;
                }
            }
            if after < line.len() {
                let next = line.as_bytes()[after];
                if next.is_ascii_alphanumeric() || next == b'_' {
                    start = after;
                    continue;
                }
            }
            // Must be preceded by `.` (member access) or appear in a
            // destructuring context (preceded by `{`, `,`, or whitespace)
            if abs > 0 {
                let prev = line.as_bytes()[abs - 1];
                if prev == b'.' || prev == b'{' || prev == b',' || prev == b' ' {
                    return Some(prop);
                }
            }
            start = after;
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if let Some(prop) = find_deprecated_key_prop(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-keyboard-event-key".into(),
                    message: format!("Use `.key` instead of `.{prop}`."),
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
    fn flags_event_keycode() {
        assert_eq!(run("if (event.keyCode === 13) {}").len(), 1);
    }

    #[test]
    fn flags_event_which() {
        assert_eq!(run("if (e.which === 27) {}").len(), 1);
    }

    #[test]
    fn flags_event_charcode() {
        assert_eq!(run("const code = event.charCode;").len(), 1);
    }

    #[test]
    fn allows_event_key() {
        assert!(run("if (event.key === 'Enter') {}").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// event.keyCode is deprecated").is_empty());
    }
}
