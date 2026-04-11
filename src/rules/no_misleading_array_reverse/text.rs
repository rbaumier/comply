use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const MUTATING_METHODS: &[&str] = &[".reverse()", ".sort()", ".fill(", ".splice("];

/// Check if a line assigns or returns the result of a mutating array method.
fn is_misleading_usage(line: &str) -> bool {
    let trimmed = line.trim();
    let has_mutating = MUTATING_METHODS.iter().any(|m| trimmed.contains(m));
    if !has_mutating {
        return false;
    }

    // Flag: `const/let/var x = expr.method()` or `return expr.method()`
    let is_assignment =
        trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ");
    let is_return = trimmed.starts_with("return ");

    // Must have both assignment/return AND a mutating method
    if !is_assignment && !is_return {
        return false;
    }

    // Make sure the mutating method is on the RHS of `=` for assignments
    if is_assignment {
        if let Some(eq_pos) = trimmed.find('=') {
            let rhs = &trimmed[eq_pos + 1..];
            if !MUTATING_METHODS.iter().any(|m| rhs.contains(m)) {
                return false;
            }
            // Allow spread copy patterns like `[...arr].reverse()`
            if rhs.contains("[...") {
                return false;
            }
            return true;
        }
        return false;
    }

    // For return statements, also allow spread copy
    if trimmed.contains("[...") {
        return false;
    }

    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_misleading_usage(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-misleading-array-reverse".into(),
                    message: "Assigning or returning the result of a mutating array method is misleading — it returns the same reference, not a copy.".into(),
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
    fn flags_const_reverse() {
        assert_eq!(run("const reversed = arr.reverse();").len(), 1);
    }

    #[test]
    fn flags_return_sort() {
        assert_eq!(run("return arr.sort();").len(), 1);
    }

    #[test]
    fn flags_let_fill() {
        assert_eq!(run("let filled = arr.fill(0);").len(), 1);
    }

    #[test]
    fn allows_standalone_call() {
        assert!(run("arr.reverse();").is_empty());
    }

    #[test]
    fn allows_spread_copy() {
        assert!(run("const reversed = [...arr].reverse();").is_empty());
    }
}
