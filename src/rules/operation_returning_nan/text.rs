use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Arithmetic operators (excluding `+` alone since string concat is valid).
const ARITH_OPS: &[&str] = &[" - ", " * ", " / "];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            // 1. `undefined` with any arithmetic operator
            for op in &["+ ", "- ", "* ", "/ "] {
                let pattern = format!("undefined {op}");
                if line.contains(&pattern) {
                    diagnostics.push(make_diag(ctx, idx, line, &pattern));
                    break;
                }
            }
            // Also check `<op> undefined` on the right side
            if diagnostics.last().is_none_or(|d| d.line != idx + 1) {
                for op in &[" +", " -", " *", " /"] {
                    let pattern = format!("{op} undefined");
                    if line.contains(&pattern) {
                        diagnostics.push(make_diag(ctx, idx, line, &pattern));
                        break;
                    }
                }
            }

            // 2. String literal with arithmetic operators (-, *, /)
            // Pattern: `"..." - `, `"..." * `, `'...' / `, or on right side
            if has_string_arithmetic(line) {
                // Avoid duplicate if we already flagged this line
                if diagnostics.last().is_none_or(|d| d.line != idx + 1) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "operation-returning-nan".into(),
                        message: "Arithmetic on a string literal will produce `NaN`.".into(),
                        severity: Severity::Error,
                    });
                }
            }
        }
        diagnostics
    }
}

fn make_diag(ctx: &CheckCtx, idx: usize, line: &str, pattern: &str) -> Diagnostic {
    let col = line.find(pattern).unwrap_or(0);
    Diagnostic {
        path: ctx.path.to_path_buf(),
        line: idx + 1,
        column: col + 1,
        rule_id: "operation-returning-nan".into(),
        message: "Arithmetic with `undefined` will produce `NaN`.".into(),
        severity: Severity::Error,
    }
}

/// Detect string literals combined with -, *, / operators.
fn has_string_arithmetic(line: &str) -> bool {
    // Find quoted strings followed by arithmetic ops
    for op in ARITH_OPS {
        // `"..." <op>` or `'...' <op>`
        for quote in &['"', '\''] {
            // string on the left: look for `"..."<op>` or `'...'<op>`
            let close_pattern = format!("{quote}{op}");
            if line.contains(&close_pattern) {
                // Verify there's a matching open quote before the close
                if let Some(close_pos) = line.find(&close_pattern) {
                    let before = &line[..close_pos];
                    if before.contains(*quote) {
                        return true;
                    }
                }
            }
            // string on the right: `<op>"..."` or `<op>'...'`
            let open_pattern = format!("{op}{quote}");
            if line.contains(&open_pattern) {
                return true;
            }
        }
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
    fn flags_undefined_plus() {
        assert_eq!(run("const x = undefined + 1;").len(), 1);
    }

    #[test]
    fn flags_undefined_minus() {
        assert_eq!(run("const x = undefined - 5;").len(), 1);
    }

    #[test]
    fn flags_string_multiply() {
        assert_eq!(run("const x = \"hello\" * 2;").len(), 1);
    }

    #[test]
    fn flags_string_minus() {
        assert_eq!(run("const x = \"text\" - 1;").len(), 1);
    }

    #[test]
    fn allows_number_arithmetic() {
        assert!(run("const x = 10 + 5;").is_empty());
    }

    #[test]
    fn allows_string_concat() {
        assert!(run("const x = \"hello\" + \" world\";").is_empty());
    }
}
