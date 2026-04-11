use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `()` (empty capturing group) inside a regex literal or `new RegExp(...)`.
fn has_empty_group(line: &str) -> bool {
    let bytes = line.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'(' && bytes[i + 1] == b')' {
            // Ensure not escaped
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 == 0 {
                // Check it's plausibly in a regex context (contains / or RegExp)
                if line.contains('/') || line.contains("RegExp") {
                    return true;
                }
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_empty_group(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-empty-group".into(),
                    message: "Empty capturing group `()` in regex — add a pattern or remove it.".into(),
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
    fn flags_empty_group_in_literal() {
        assert_eq!(run("const re = /foo()/;").len(), 1);
    }

    #[test]
    fn flags_empty_group_in_regexp() {
        assert_eq!(run("const re = new RegExp(\"foo()\");").len(), 1);
    }

    #[test]
    fn allows_non_empty_group() {
        assert!(run("const re = /foo(bar)/;").is_empty());
    }
}
