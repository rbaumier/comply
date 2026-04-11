use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `= undefined` assignments (with optional semicolon/trailing content).
/// Matches: `let x = undefined`, `x = undefined;`, `const x: Foo = undefined`.
fn has_undefined_assignment(line: &str) -> bool {
    let trimmed = line.trim();
    // Skip `== undefined` and `!= undefined` (comparisons, not assignments).
    // Also skip `=== undefined` and `!== undefined`.
    let mut start = 0;
    while let Some(pos) = trimmed[start..].find("= undefined") {
        let abs = start + pos;
        // Check this is a plain `=`, not `==`, `!=`, `===`, `!==`.
        if abs > 0 {
            let prev = trimmed.as_bytes()[abs - 1];
            if prev == b'=' || prev == b'!' {
                start = abs + 11;
                continue;
            }
        }
        // Check it's not `=== undefined`.
        let after_eq = &trimmed[abs..];
        if after_eq.starts_with("== ") || after_eq.starts_with("==u") {
            start = abs + 11;
            continue;
        }
        // Verify `undefined` is the full token (not `undefinedValue`).
        let after = abs + 12; // len of "= undefined"
        if after < trimmed.len() {
            let next_ch = trimmed.as_bytes()[after];
            if next_ch.is_ascii_alphanumeric() || next_ch == b'_' {
                start = abs + 11;
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
            if has_undefined_assignment(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-undefined-assignment".into(),
                    message:
                        "Do not assign `undefined` — use `let x;` or `delete obj.prop` instead."
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
    fn flags_let_undefined() {
        assert_eq!(run("let x = undefined;").len(), 1);
    }

    #[test]
    fn flags_reassignment_undefined() {
        assert_eq!(run("x = undefined;").len(), 1);
    }

    #[test]
    fn allows_comparison_equals() {
        assert!(run("if (x == undefined) {}").is_empty());
    }

    #[test]
    fn allows_strict_comparison() {
        assert!(run("if (x === undefined) {}").is_empty());
    }

    #[test]
    fn allows_not_equals() {
        assert!(run("if (x !== undefined) {}").is_empty());
    }
}
