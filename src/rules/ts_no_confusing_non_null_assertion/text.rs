//! ts-no-confusing-non-null-assertion backend — scan for `! ==`, `! ===`,
//! `! =` patterns that look confusingly like `!==`, `!=`.
//!
//! We also match `! instanceof` and `! in` per the original rule.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // Match `!` followed by whitespace and then `=`, `==`, `===`, `in`, or `instanceof`
    // but NOT `!=` or `!==` (those are the correct operators)
    Regex::new(r"!\s+(===?|=|in\b|instanceof\b)").unwrap()
});

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for m in PATTERN.find_iter(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: m.start() + 1,
                    rule_id: "ts-no-confusing-non-null-assertion".into(),
                    message: "Confusing non-null assertion before comparison — \
                              `a! == b` looks like `a !== b`. Remove the `!` or \
                              wrap in parentheses."
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
    fn flags_non_null_before_equality() {
        let diags = run("if (a! == b) {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_non_null_before_strict_equality() {
        let diags = run("if (a! === b) {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_proper_not_equal() {
        assert!(run("if (a !== b) {}").is_empty());
    }

    #[test]
    fn allows_proper_not_strict_equal() {
        assert!(run("if (a != b) {}").is_empty());
    }
}
