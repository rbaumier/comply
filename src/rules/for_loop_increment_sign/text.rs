use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects for loops where the increment direction conflicts with the condition.
/// Flags patterns like `for (... i < ... ; i--` or `for (... i > ... ; i++`.
fn has_wrong_increment(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with("for ") && !trimmed.starts_with("for(") {
        return false;
    }

    // Find the content inside the `for (...)`.
    let open = match trimmed.find('(') {
        Some(p) => p,
        None => return false,
    };
    let close = match trimmed.rfind(')') {
        Some(p) => p,
        None => return false,
    };
    if open >= close {
        return false;
    }
    let inner = &trimmed[open + 1..close];

    // Split by `;` to get the three parts.
    let parts: Vec<&str> = inner.split(';').collect();
    if parts.len() < 3 {
        return false;
    }
    let condition = parts[1].trim();
    let increment = parts[2].trim();

    // Check for `< ... ; ...--` or `> ... ; ...++`.
    let has_less_than = condition.contains('<');
    let has_greater_than = condition.contains('>');
    let has_increment = increment.contains("++");
    let has_decrement = increment.contains("--");

    // `i < N ; i--` is wrong (will never terminate or go wrong direction)
    if has_less_than && !has_greater_than && has_decrement && !has_increment {
        return true;
    }
    // `i > N ; i++` is wrong
    if has_greater_than && !has_less_than && has_increment && !has_decrement {
        return true;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_wrong_increment(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "for-loop-increment-sign".into(),
                    message: "For-loop increment direction conflicts with condition — loop may be infinite or never execute.".into(),
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
    fn flags_less_than_with_decrement() {
        assert_eq!(run("for (let i = 0; i < 10; i--) {}").len(), 1);
    }

    #[test]
    fn flags_greater_than_with_increment() {
        assert_eq!(run("for (let i = 10; i > 0; i++) {}").len(), 1);
    }

    #[test]
    fn allows_less_than_with_increment() {
        assert!(run("for (let i = 0; i < 10; i++) {}").is_empty());
    }

    #[test]
    fn allows_greater_than_with_decrement() {
        assert!(run("for (let i = 10; i > 0; i--) {}").is_empty());
    }
}
