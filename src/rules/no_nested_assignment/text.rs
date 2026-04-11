use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Check if a line contains an assignment inside a conditional context.
/// Looks for `if (`, `while (`, `for (` lines that contain a single `=`
/// that is not part of `==`, `===`, `!=`, `!==`, `<=`, `>=`, or `=>`.
fn has_nested_assignment(line: &str) -> bool {
    let trimmed = line.trim();

    // Must be a conditional line
    let is_conditional = trimmed.starts_with("if (")
        || trimmed.starts_with("if(")
        || trimmed.starts_with("} else if (")
        || trimmed.starts_with("} else if(")
        || trimmed.starts_with("while (")
        || trimmed.starts_with("while(");

    if !is_conditional {
        return false;
    }

    // Find the parenthesized condition
    let paren_start = match trimmed.find('(') {
        Some(pos) => pos + 1,
        None => return false,
    };
    let condition = &trimmed[paren_start..];

    contains_bare_assignment(condition)
}

/// Returns true if `s` contains a `=` that is NOT part of
/// `==`, `===`, `!=`, `!==`, `<=`, `>=`, `=>`.
fn contains_bare_assignment(s: &str) -> bool {
    let bytes = s.as_bytes();
    let len = bytes.len();

    for i in 0..len {
        if bytes[i] != b'=' {
            continue;
        }

        // Check what's after: skip `==`, `===`, `=>`
        if i + 1 < len && (bytes[i + 1] == b'=' || bytes[i + 1] == b'>') {
            continue;
        }

        // Check what's before: skip `!=`, `!==`, `<=`, `>=`
        if i > 0 {
            let prev = bytes[i - 1];
            if prev == b'!' || prev == b'<' || prev == b'>' || prev == b'=' {
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
            if has_nested_assignment(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-nested-assignment".into(),
                    message: "Assignment inside a condition — likely a bug, use `===` for comparison or move the assignment out.".into(),
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
    fn flags_assignment_in_if() {
        assert_eq!(run("if (x = 10) {}").len(), 1);
    }

    #[test]
    fn flags_assignment_in_while() {
        assert_eq!(run("while (node = node.next) {}").len(), 1);
    }

    #[test]
    fn allows_equality_check() {
        assert!(run("if (x === 10) {}").is_empty());
    }

    #[test]
    fn allows_loose_equality() {
        assert!(run("if (x == 10) {}").is_empty());
    }

    #[test]
    fn allows_not_equal() {
        assert!(run("if (x !== 10) {}").is_empty());
    }

    #[test]
    fn allows_comparison_operators() {
        assert!(run("if (x <= 10) {}").is_empty());
        assert!(run("if (x >= 10) {}").is_empty());
    }

    #[test]
    fn allows_arrow_function() {
        assert!(run("if (items.filter(x => x > 0).length) {}").is_empty());
    }
}
