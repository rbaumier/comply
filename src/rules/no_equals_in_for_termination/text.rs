use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(col) = has_equality_in_for_condition(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "no-equals-in-for-termination".into(),
                    message: "`for` loop uses equality (`==`/`===`) in termination — use `<`, `<=`, `>`, or `>=` instead.".into(),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

/// If the line contains a classic `for (init; cond; update)` with `===` or `==`
/// in the condition part, return the column of `for`.
fn has_equality_in_for_condition(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return None;
    }
    let mut start = 0;
    while let Some(for_pos) = line[start..].find("for") {
        let abs = start + for_pos;
        // Ensure `for` is not part of a longer identifier
        if abs > 0 && line.as_bytes()[abs - 1].is_ascii_alphanumeric() {
            start = abs + 3;
            continue;
        }
        let after = abs + 3;
        // After `for` we expect optional whitespace then `(`
        let rest = &line[after..];
        let rest_trimmed = rest.trim_start();
        if !rest_trimmed.starts_with('(') {
            start = abs + 3;
            continue;
        }
        let paren_offset = after + (rest.len() - rest_trimmed.len());
        // Find the condition part between the first `;` and the second `;`
        if let Some(first_semi) = line[paren_offset..].find(';') {
            let cond_start = paren_offset + first_semi + 1;
            if let Some(second_semi) = line[cond_start..].find(';') {
                let condition = &line[cond_start..cond_start + second_semi];
                // Check for `===` or `==` (but not `!==` or `!=`)
                if contains_equality_op(condition) {
                    return Some(abs);
                }
            }
        }
        start = abs + 3;
    }
    None
}

/// Returns true if the string contains `===` or `==` but not `!==` or `!=`.
fn contains_equality_op(s: &str) -> bool {
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'=' {
            // Check for `===`
            if i + 2 < bytes.len() && bytes[i + 1] == b'=' && bytes[i + 2] == b'=' {
                // Make sure it's not `!==`
                let not_negated = i == 0 || bytes[i - 1] != b'!';
                if not_negated {
                    return true;
                }
                i += 3;
                continue;
            }
            // Check for `==` (but not `===` which we handled, and not `=` assignment)
            if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                // Make sure the next char is not `=` (already handled above)
                let triple = i + 2 < bytes.len() && bytes[i + 2] == b'=';
                let not_negated = i == 0 || bytes[i - 1] != b'!';
                if !triple && not_negated {
                    return true;
                }
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_triple_equals() {
        assert_eq!(run("for (let i = 0; i === 10; i++) {}").len(), 1);
    }

    #[test]
    fn flags_double_equals() {
        assert_eq!(run("for (let i = 0; i == 10; i++) {}").len(), 1);
    }

    #[test]
    fn allows_less_than() {
        assert!(run("for (let i = 0; i < 10; i++) {}").is_empty());
    }

    #[test]
    fn allows_less_than_or_equal() {
        assert!(run("for (let i = 0; i <= 10; i++) {}").is_empty());
    }

    #[test]
    fn allows_not_equals() {
        assert!(run("for (let i = 0; i !== 10; i++) {}").is_empty());
    }
}
