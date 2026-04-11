use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("new Array(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-array-constructor".into(),
                    message: "Avoid `new Array()` — use array literals `[]` instead.".into(),
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
    fn flags_new_array_numeric() {
        assert_eq!(run("const a = new Array(3);").len(), 1);
    }

    #[test]
    fn flags_new_array_with_elements() {
        assert_eq!(run("const a = new Array(1, 2, 3);").len(), 1);
    }

    #[test]
    fn allows_array_literal() {
        assert!(run("const a = [1, 2, 3];").is_empty());
    }

    #[test]
    fn allows_array_from() {
        assert!(run("const a = Array.from({ length: 3 });").is_empty());
    }
}
