use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Known obscure ranges that cross ASCII groups and include unexpected chars.
const OBSCURE_RANGES: &[&str] = &[
    "A-z", // includes [\]^_`
    "a-Z", // reversed / nonsensical
    "0-z", // digits + uppercase + symbols + lowercase
    "0-Z", // digits + symbols + uppercase
];

fn has_obscure_range(line: &str) -> bool {
    if !line.contains('/') && !line.contains("RegExp") && !line.contains("Regex::") {
        return false;
    }
    for range in OBSCURE_RANGES {
        if line.contains(range) {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_obscure_range(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-no-obscure-range".into(),
                    message: "Character class range crosses ASCII groups (e.g. `[A-z]`) — use `[A-Za-z]` instead.".into(),
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
    fn flags_a_to_z_upper_lower() {
        assert_eq!(run("const re = /[A-z]/;").len(), 1);
    }

    #[test]
    fn flags_zero_to_z() {
        assert_eq!(run("const re = /[0-z]/;").len(), 1);
    }

    #[test]
    fn allows_proper_range() {
        assert!(run("const re = /[A-Za-z]/;").is_empty());
    }

    #[test]
    fn allows_digit_range() {
        assert!(run("const re = /[0-9]/;").is_empty());
    }
}
