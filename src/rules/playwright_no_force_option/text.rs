//! playwright-no-force-option text backend — flag `force: true` on Playwright actions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Playwright actions that accept a `force` option.
const FORCE_ACTIONS: &[&str] = &[
    ".click(",
    ".fill(",
    ".hover(",
    ".check(",
    ".uncheck(",
    ".selectOption(",
    ".dblclick(",
    ".tap(",
    ".press(",
    ".dragTo(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Only check lines that contain a Playwright action.
            let has_action = FORCE_ACTIONS.iter().any(|a| line.contains(a));
            if !has_action {
                continue;
            }
            // Check for `force: true` or `force:true` on the same line.
            let stripped = line.replace(' ', "");
            if let Some(col) = stripped.find("force:true") {
                // Report column from original line.
                let original_col = line.find("force").unwrap_or(col);
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: original_col + 1,
                    rule_id: "playwright-no-force-option".into(),
                    message: "`force: true` bypasses actionability checks — \
                              fix the underlying page state instead."
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

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_force_true_on_click() {
        let diags = run(
            "login.test.ts",
            "await page.click('#btn', { force: true });",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "playwright-no-force-option");
    }

    #[test]
    fn flags_force_true_on_fill() {
        let diags = run(
            "form.spec.ts",
            "await input.fill('hello', { force: true });",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_force_true_no_spaces() {
        let diags = run(
            "nav.test.ts",
            "await page.hover('.menu', {force:true});",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_click_without_force() {
        let diags = run("login.test.ts", "await page.click('#btn');");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_force_false() {
        let diags = run(
            "login.test.ts",
            "await page.click('#btn', { force: false });",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run(
            "helpers.ts",
            "await page.click('#btn', { force: true });",
        );
        assert!(diags.is_empty());
    }
}
