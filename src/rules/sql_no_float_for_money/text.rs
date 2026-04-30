use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const MONEY_WORDS: &[&str] = &[
    "price", "amount", "cost", "total", "balance", "fee", "tax", "revenue", "salary", "budget",
    "payment", "invoice",
];
const FLOAT_TYPES: &[&str] = &["FLOAT", "DOUBLE", "REAL"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            let has_money = MONEY_WORDS.iter().any(|w| lower.contains(w));
            if !has_money {
                continue;
            }
            let upper = line.to_ascii_uppercase();
            if let Some(ft) = FLOAT_TYPES.iter().find(|t| upper.contains(*t)) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc), line: idx + 1, column: 1,
                    rule_id: "sql-no-float-for-money".into(),
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
