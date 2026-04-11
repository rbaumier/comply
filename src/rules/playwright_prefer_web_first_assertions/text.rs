//! playwright-prefer-web-first-assertions text backend — flag `expect(await locator.isVisible())` etc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Playwright locator methods that have web-first assertion equivalents.
const LOCATOR_METHODS: &[&str] = &[
    "isVisible(",
    "isHidden(",
    "isEnabled(",
    "isDisabled(",
    "isChecked(",
    "isEditable(",
    "textContent(",
    "innerText(",
    "innerHTML(",
    "getAttribute(",
    "inputValue(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Look for `expect(await` pattern.
            if let Some(col) = line.find("expect(await") {
                // Check if any known locator method follows on this line.
                let rest = &line[col..];
                let has_locator_method = LOCATOR_METHODS.iter().any(|m| rest.contains(m));
                if has_locator_method {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: "playwright-prefer-web-first-assertions".into(),
                        message: "Use web-first assertions (`toBeVisible`, \
                                  `toBeEnabled`, etc.) instead of asserting on \
                                  awaited locator methods — they auto-retry."
                            .into(),
                        severity: Severity::Warning,
                    });
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
    fn flags_is_visible_assertion() {
        let diags = run(
            "login.test.ts",
            "expect(await page.locator('#btn').isVisible()).toBe(true);",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "playwright-prefer-web-first-assertions");
    }

    #[test]
    fn flags_text_content_assertion() {
        let diags = run(
            "header.spec.ts",
            "expect(await el.textContent()).toContain('Hello');",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_input_value_assertion() {
        let diags = run(
            "form.test.ts",
            "expect(await input.inputValue()).toBe('test');",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_web_first_assertion() {
        let diags = run(
            "login.test.ts",
            "await expect(page.locator('#btn')).toBeVisible();",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_expect_await_with_non_locator() {
        let diags = run(
            "api.test.ts",
            "expect(await fetch('/api')).toBeDefined();",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run(
            "helpers.ts",
            "expect(await el.isVisible()).toBe(true);",
        );
        assert!(diags.is_empty());
    }
}
