use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(ft) = super::float_type_for_money_line(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc), line: idx + 1, column: 1,
                    rule_id: super::META.id.into(),
                    message: format!("`{ft}` near a monetary column — use `NUMERIC(precision, scale)` to avoid floating-point rounding errors."),
                    severity: Severity::Error,
                    span: None,
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
        Check.check(&CheckCtx::for_test(Path::new("t.sql"), source))
    }

    #[test]
    fn flags() {
        assert_eq!(run("price FLOAT NOT NULL").len(), 1);
    }
    #[test]
    fn allows_numeric() {
        assert!(run("price NUMERIC(10, 2) NOT NULL").is_empty());
    }
}
