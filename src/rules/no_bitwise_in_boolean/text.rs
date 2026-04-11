use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract the condition portion from boolean-context keywords like `if (...)`, `while (...)`.
fn extract_condition(line: &str) -> Option<&str> {
    for keyword in &["if", "while"] {
        if let Some(pos) = line.find(keyword) {
            let after_kw = &line[pos + keyword.len()..];
            let trimmed = after_kw.trim_start();
            if trimmed.starts_with('(') {
                return Some(trimmed);
            }
        }
    }
    None
}

/// Check if a condition string contains a standalone bitwise operator.
/// We look for ` & `, ` | `, ` ^ ` (surrounded by spaces) or `~` to avoid
/// matching `&&`, `||`, or operators inside other tokens.
fn has_bitwise_op(cond: &str) -> bool {
    // Check for ~ (unary, no spacing needed)
    if cond.contains('~') {
        return true;
    }
    // Check for single & not part of &&
    let bytes = cond.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'&' => {
                let next = bytes.get(i + 1).copied();
                let prev = if i > 0 { Some(bytes[i - 1]) } else { None };
                if next != Some(b'&') && prev != Some(b'&') {
                    return true;
                }
            }
            b'|' => {
                let next = bytes.get(i + 1).copied();
                let prev = if i > 0 { Some(bytes[i - 1]) } else { None };
                if next != Some(b'|') && prev != Some(b'|') {
                    return true;
                }
            }
            b'^' => return true,
            _ => {}
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(cond) = extract_condition(line)
                && has_bitwise_op(cond) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-bitwise-in-boolean".into(),
                        message: "Bitwise operator in boolean context — did you mean `&&` or `||`?"
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
    fn flags_bitwise_and_in_if() {
        assert_eq!(run("if (x & y) {}").len(), 1);
    }

    #[test]
    fn flags_bitwise_or_in_if() {
        assert_eq!(run("if (x | y) {}").len(), 1);
    }

    #[test]
    fn flags_bitwise_xor_in_while() {
        assert_eq!(run("while (a ^ b) {}").len(), 1);
    }

    #[test]
    fn flags_bitwise_not_in_if() {
        assert_eq!(run("if (~mask) {}").len(), 1);
    }

    #[test]
    fn allows_logical_and() {
        assert!(run("if (x && y) {}").is_empty());
    }

    #[test]
    fn allows_logical_or() {
        assert!(run("if (x || y) {}").is_empty());
    }

    #[test]
    fn allows_bitwise_outside_condition() {
        assert!(run("const mask = a & b;").is_empty());
    }
}
