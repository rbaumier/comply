//! prefer-set-size — flag `[...set].length` and `Array.from(set).length`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

/// Matches `[...identifier].length` — the spread-into-array-then-length pattern.
static SPREAD_LENGTH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\.\.\.\s*\w+\s*\]\.length\b").unwrap());

/// Matches `Array.from(identifier).length` — the Array.from-then-length pattern.
static ARRAY_FROM_LENGTH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Array\.from\(\s*\w+\s*\)\.length\b").unwrap());

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if SPREAD_LENGTH.is_match(line) || ARRAY_FROM_LENGTH.is_match(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-set-size".into(),
                    message: "Prefer `Set#size` instead of `[...set].length` or `Array.from(set).length`.".into(),
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
    fn flags_spread_length() {
        let d = run("const len = [...mySet].length;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-set-size");
    }

    #[test]
    fn flags_array_from_length() {
        let d = run("const len = Array.from(mySet).length;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_set_size() {
        assert!(run("const len = mySet.size;").is_empty());
    }

    #[test]
    fn allows_array_spread_without_length() {
        assert!(run("const arr = [...mySet];").is_empty());
    }

    #[test]
    fn allows_regular_array_length() {
        assert!(run("const len = myArray.length;").is_empty());
    }
}
