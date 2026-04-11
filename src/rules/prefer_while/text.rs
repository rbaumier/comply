use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects `for (;;)` or `for (;condition;)` — no init, no update.
fn is_for_without_init_update(line: &str) -> bool {
    let trimmed = line.trim();

    // Find `for (`  or  `for(`
    let rest = if let Some(r) = trimmed.strip_prefix("for (") {
        r
    } else if let Some(r) = trimmed.strip_prefix("for(") {
        r
    } else {
        return false;
    };

    // The init part (before first `;`) must be empty (just whitespace).
    let Some(first_semi) = rest.find(';') else {
        return false;
    };
    let init = rest[..first_semi].trim();
    if !init.is_empty() {
        return false;
    }

    // The update part (between second `;` and `)`) must be empty.
    let after_first = &rest[first_semi + 1..];
    let Some(second_semi) = after_first.find(';') else {
        return false;
    };
    let after_second = &after_first[second_semi + 1..];

    // Find the closing paren — everything between second `;` and `)` is the update.
    let Some(close_paren) = after_second.find(')') else {
        return false;
    };
    let update = after_second[..close_paren].trim();
    if !update.is_empty() {
        return false;
    }

    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_for_without_init_update(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-while".into(),
                    message: "Use `while` instead of `for` without init/update.".into(),
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
    fn flags_for_infinite() {
        assert_eq!(run("for (;;) {").len(), 1);
    }

    #[test]
    fn flags_for_condition_only() {
        assert_eq!(run("for (;x < 10;) {").len(), 1);
    }

    #[test]
    fn allows_standard_for_loop() {
        assert!(run("for (let i = 0; i < 10; i++) {").is_empty());
    }

    #[test]
    fn allows_while_true() {
        assert!(run("while (true) {").is_empty());
    }
}
