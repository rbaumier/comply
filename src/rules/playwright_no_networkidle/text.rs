//! playwright-no-networkidle text backend — flag `"networkidle"` string argument.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Patterns to search for: both single- and double-quoted variants.
const NETWORKIDLE_PATTERNS: &[&str] = &[
    "\"networkidle\"",
    "'networkidle'",
    "`networkidle`",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for pattern in NETWORKIDLE_PATTERNS {
                if let Some(col) = line.find(pattern) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: "playwright-no-networkidle".into(),
                        message: "`networkidle` is timing-based and flaky — \
                                  use a web-first assertion or \
                                  `waitForResponse` instead."
                            .into(),
                        severity: Severity::Warning,
                    });
                    break; // One diagnostic per line.
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_networkidle_in_goto() {
        let diags = run(
            "nav.test.ts",
            "await page.goto('/', { waitUntil: \"networkidle\" });",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "playwright-no-networkidle");
    }

    #[test]
    fn flags_networkidle_single_quotes() {
        let diags = run(
            "nav.spec.ts",
            "await page.waitForLoadState('networkidle');",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_domcontentloaded() {
        let diags = run(
            "nav.test.ts",
            "await page.goto('/', { waitUntil: \"domcontentloaded\" });",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run(
            "utils.ts",
            "await page.goto('/', { waitUntil: \"networkidle\" });",
        );
        assert!(diags.is_empty());
    }
}
