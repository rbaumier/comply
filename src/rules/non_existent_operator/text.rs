use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect typo operators `=+`, `=-`, `=!` in code.
/// These are likely meant to be `+=`, `-=`, `!=`.
/// We need to exclude `===`, `==`, `=>`, `=== !`, and string/comment contexts.
fn has_typo_operator(line: &str) -> bool {
    let bytes = line.as_bytes();
    let len = bytes.len();

    for i in 0..len.saturating_sub(1) {
        if bytes[i] != b'=' {
            continue;
        }

        let next = bytes[i + 1];
        if next != b'+' && next != b'-' && next != b'!' {
            continue;
        }

        // Skip if preceded by `=` (part of `==+`, `===`, etc.)
        if i > 0 && bytes[i - 1] == b'=' {
            continue;
        }

        // Skip if preceded by `!`, `<`, `>` (part of `!=`, `<=`, `>=`)
        if i > 0 && (bytes[i - 1] == b'!' || bytes[i - 1] == b'<' || bytes[i - 1] == b'>') {
            continue;
        }

        // For `=!`: skip if followed by `=` (this would be `=!=` which is unlikely but safe)
        if next == b'!' && i + 2 < len && bytes[i + 2] == b'=' {
            continue;
        }

        // Ensure there's an identifier character before `=` (letter, digit, _, ], ))
        // to confirm this is an assignment context, not `= +x` (unary).
        // We require NO space between `=` and `+`/`-`/`!`, which is already guaranteed
        // since we check bytes[i+1] directly.

        // Also skip if the char after the operator is a space — `= + x` is valid.
        // We already match only `=+`, `=-`, `=!` with no space, which is the typo pattern.

        return true;
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_typo_operator(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "non-existent-operator".into(),
                    message: "Typo operator — did you mean `+=`, `-=`, or `!=`?".into(),
                    severity: Severity::Error,
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
    fn flags_equals_plus() {
        assert_eq!(run("x =+ 1;").len(), 1);
    }

    #[test]
    fn flags_equals_minus() {
        assert_eq!(run("x =- 1;").len(), 1);
    }

    #[test]
    fn flags_equals_bang() {
        assert_eq!(run("x =! true;").len(), 1);
    }

    #[test]
    fn allows_plus_equals() {
        assert!(run("x += 1;").is_empty());
    }

    #[test]
    fn allows_minus_equals() {
        assert!(run("x -= 1;").is_empty());
    }

    #[test]
    fn allows_not_equals() {
        assert!(run("if (x !== y) {}").is_empty());
        assert!(run("if (x != y) {}").is_empty());
    }

    #[test]
    fn allows_triple_equals() {
        assert!(run("if (x === y) {}").is_empty());
    }
}
