//! playwright-no-element-handle text backend — flag `page.$()` and `page.$$()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Check for `.$$(` first (longer match), then `.$(`.
            for (pattern, label) in &[(".$$(",  "page.$$()"), (".$(",  "page.$()")] {
                if let Some(col) = line.find(pattern) {
                    // Verify `page` precedes the pattern on this line.
                    let prefix = &line[..col];
                    if prefix.contains("page") {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: col + 1,
                            rule_id: "playwright-no-element-handle".into(),
                            message: format!(
                                "`{label}` returns a deprecated ElementHandle — \
                                 use `page.locator()` instead."
                            ),
                            severity: Severity::Warning,
                        });
                        break; // One diagnostic per line.
                    }
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
    fn flags_page_dollar() {
        let diags = run(
            "login.test.ts",
            "const el = await page.$('.btn');",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "playwright-no-element-handle");
    }

    #[test]
    fn flags_page_dollar_dollar() {
        let diags = run(
            "list.spec.ts",
            "const items = await page.$$('li');",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_page_locator() {
        let diags = run(
            "login.test.ts",
            "const el = page.locator('.btn');",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_page_dollar() {
        // `frame.$()` is not flagged (only `page`).
        let diags = run("login.test.ts", "const el = await frame.$('.btn');");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run("helpers.ts", "const el = await page.$('.btn');");
        assert!(diags.is_empty());
    }
}
