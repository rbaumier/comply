use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("Math.random(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-pseudo-random".into(),
                    message: "`Math.random()` is not cryptographically secure — use `crypto.randomUUID()` or `crypto.getRandomValues()`.".into(),
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
    fn flags_math_random() {
        assert_eq!(run("const x = Math.random();").len(), 1);
    }

    #[test]
    fn flags_math_random_in_expression() {
        assert_eq!(run("const id = Math.floor(Math.random() * 1000);").len(), 1);
    }

    #[test]
    fn allows_crypto_random() {
        assert!(run("const id = crypto.randomUUID();").is_empty());
    }

    #[test]
    fn allows_get_random_values() {
        assert!(run("crypto.getRandomValues(new Uint8Array(16));").is_empty());
    }
}
