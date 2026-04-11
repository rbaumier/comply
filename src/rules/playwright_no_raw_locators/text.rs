//! playwright-no-raw-locators text backend — flag `.locator()` with CSS selectors.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Characters that indicate a CSS selector rather than a text/role locator.
const CSS_INDICATOR_CHARS: &[char] = &['.', '#', '[', '>', ':', '+', '~'];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(col) = line.find(".locator(") {
                let rest = &line[col + ".locator(".len()..];
                // Extract the string argument (single or double quoted).
                let selector = extract_string_arg(rest);
                if let Some(sel) = selector
                    && sel.chars().any(|c| CSS_INDICATOR_CHARS.contains(&c))
                {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: "playwright-no-raw-locators".into(),
                        message: "Raw CSS selector in `.locator()` — prefer \
                                  `getByRole`, `getByText`, or other \
                                  semantic locators."
                            .into(),
                        severity: Severity::Warning,
                    });
                }
            }
        }
        diagnostics
    }
}

/// Extract a string argument from the beginning of the rest of the line
/// (after `.locator(`). Returns the content between quotes if found.
fn extract_string_arg(s: &str) -> Option<&str> {
    let trimmed = s.trim_start();
    let quote = trimmed.chars().next()?;
    if quote != '\'' && quote != '"' && quote != '`' {
        return None;
    }
    let inner = &trimmed[1..];
    let end = inner.find(quote)?;
    Some(&inner[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_class_selector() {
        let diags = run(
            "login.test.ts",
            "page.locator('.submit-btn');",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "playwright-no-raw-locators");
    }

    #[test]
    fn flags_id_selector() {
        let diags = run("login.test.ts", "page.locator('#login-form');");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_attribute_selector() {
        let diags = run(
            "form.spec.ts",
            "page.locator('[data-cy=submit]');",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_child_combinator() {
        let diags = run("nav.test.ts", "page.locator('div > span');");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_text_locator() {
        // Plain text without CSS indicators.
        let diags = run("login.test.ts", "page.locator('Submit');");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_get_by_role() {
        let diags = run("login.test.ts", "page.getByRole('button');");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run("helpers.ts", "page.locator('.btn');");
        assert!(diags.is_empty());
    }
}
