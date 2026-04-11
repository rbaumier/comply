use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.matches("${").count() >= 2 {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-nested-template-literal".into(),
                    message: "Nested template literal — extract the inner expression to a named variable.".into(),
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
    fn flags_nested() {
        assert_eq!(run(r#"const msg = `Hello ${user.name}, you have ${`${count} items`}`;"#).len(), 1);
    }

    #[test]
    fn allows_single_interpolation() {
        assert!(run(r#"const msg = `Hello ${name}`;"#).is_empty());
    }

    #[test]
    fn allows_no_interpolation() {
        assert!(run(r#"const msg = `plain string`;"#).is_empty());
    }
}
