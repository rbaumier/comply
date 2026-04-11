use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PROPERTIES: &[&str] = &[".length", ".size"];
const BAD_OPERATORS: &[&str] = &[">= 0", "< 0"];

/// Check if the line contains `.length >= 0`, `.length < 0`, `.size >= 0`, `.size < 0`.
fn has_mischeck(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    for prop in PROPERTIES {
        for &op in BAD_OPERATORS {
            // Look for `.length >= 0` etc. — prop immediately followed by whitespace and operator
            let pattern = format!("{} {}", prop, op);
            if trimmed.contains(&pattern) {
                let desc = if op == ">= 0" { "always true" } else { "always false" };
                return Some(desc);
            }
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(desc) = has_mischeck(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-collection-size-mischeck".into(),
                    message: format!("This collection size check is {} — `.length` and `.size` are never negative.", desc),
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
    fn flags_length_gte_zero() {
        assert_eq!(run("if (arr.length >= 0) {}").len(), 1);
    }

    #[test]
    fn flags_length_lt_zero() {
        assert_eq!(run("if (arr.length < 0) {}").len(), 1);
    }

    #[test]
    fn flags_size_gte_zero() {
        assert_eq!(run("if (set.size >= 0) {}").len(), 1);
    }

    #[test]
    fn allows_length_gt_zero() {
        assert!(run("if (arr.length > 0) {}").is_empty());
    }

    #[test]
    fn allows_length_eq_zero() {
        assert!(run("if (arr.length === 0) {}").is_empty());
    }
}
