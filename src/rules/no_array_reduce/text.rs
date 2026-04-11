use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const METHODS: &[&str] = &[".reduce(", ".reduceRight("];

/// Detect `.reduce(` or `.reduceRight(` calls on a line.
fn has_reduce_call(line: &str) -> Option<&'static str> {
    METHODS.iter().find(|&method| line.contains(method)).map(|v| v as _)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(method) = has_reduce_call(line) {
                let name = method.trim_matches(|c| c == '.' || c == '(');
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-array-reduce".into(),
                    message: format!(
                        "`Array#{}()` is not allowed — use a `for` loop or other array methods for better readability.",
                        name
                    ),
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
    fn flags_reduce() {
        assert_eq!(run("const sum = arr.reduce((acc, x) => acc + x, 0);").len(), 1);
    }

    #[test]
    fn flags_reduce_right() {
        assert_eq!(run("const r = arr.reduceRight((acc, x) => acc + x, 0);").len(), 1);
    }

    #[test]
    fn allows_non_reduce() {
        assert!(run("const x = arr.map(x => x * 2);").is_empty());
    }

    #[test]
    fn allows_unrelated_reduce_word() {
        assert!(run("// We need to reduce complexity").is_empty());
    }
}
