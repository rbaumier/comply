//! no-page-click-deprecated backend — detect deprecated Playwright direct
//! page methods (`page.click`, `page.fill`, etc.) in test files.
//!
//! Why: Playwright deprecated the direct `page.<action>(selector)` methods
//! in favour of the locator API (`page.locator(selector).<action>()`).
//! The locator API auto-waits and auto-retries, making tests more
//! resilient. The old methods will be removed in a future major version.
//!
//! Detection: per-line substring scan for `page.click(`, `page.fill(`,
//! `page.type(`, `page.check(`, `page.uncheck(`. Only fires in test
//! files (path contains `.test.`, `.spec.`, `__tests__`, `_test.`,
//! `.e2e.`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// Deprecated direct page methods we want to flag.
const DEPRECATED_METHODS: &[&str] = &[
    "page.click(",
    "page.fill(",
    "page.type(",
    "page.check(",
    "page.uncheck(",
];

/// Path fragments that identify test files.
const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path_str = ctx.path.to_string_lossy();
        if !TEST_MARKERS.iter().any(|m| path_str.contains(m)) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(method) = find_deprecated_call(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-page-click-deprecated".into(),
                    message: format!(
                        "Deprecated Playwright method `{method})` — use \
                         `page.locator(selector).{}` instead.",
                        method.strip_prefix("page.").unwrap_or(method),
                    ),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

/// Returns the matched deprecated call (e.g. `"page.click("`) if found on
/// this line, or `None` when the line is clean.
fn find_deprecated_call(line: &str) -> Option<&'static str> {
    DEPRECATED_METHODS.iter().find(|&&method| line.contains(method)).copied().map(|v| v as _)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_page_click_in_test() {
        let diags = run("login.test.ts", "await page.click('#btn');");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-page-click-deprecated");
    }

    #[test]
    fn flags_page_fill_in_spec() {
        let diags = run("form.spec.ts", "await page.fill('#email', 'a@b.c');");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_page_type_in_e2e() {
        let diags = run("checkout.e2e.ts", "await page.type('#search', 'hello');");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_page_check_in_test() {
        let diags = run("settings_test.ts", "await page.check('#agree');");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_page_uncheck_in_test() {
        let diags = run("settings_test.ts", "await page.uncheck('#agree');");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_locator_api() {
        let diags = run("login.test.ts", "await page.locator('#btn').click();");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_wait_for() {
        let diags = run("login.test.ts", "await page.waitForSelector('#btn');");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run("utils.ts", "await page.click('#btn');");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_prod_code() {
        let diags = run("src/lib/helpers.ts", "page.click('#submit');");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_in_dunder_tests_dir() {
        let diags = run(
            "src/__tests__/integration.ts",
            "await page.click('#nav');",
        );
        assert_eq!(diags.len(), 1);
    }
}
