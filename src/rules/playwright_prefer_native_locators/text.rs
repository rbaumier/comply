//! playwright-prefer-native-locators text backend — flag `locator('[role="..."]')` etc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Attribute selectors inside `.locator()` that have native equivalents.
const ATTRIBUTE_SELECTORS: &[(&str, &str)] = &[
    ("[role=", "getByRole"),
    ("[placeholder=", "getByPlaceholder"),
    ("[alt=", "getByAltText"),
    ("[title=", "getByTitle"),
    ("[data-testid=", "getByTestId"),
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(col) = line.find(".locator(") {
                let rest = &line[col + ".locator(".len()..];
                for &(attr, replacement) in ATTRIBUTE_SELECTORS {
                    if rest.contains(attr) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: col + 1,
                            rule_id: "playwright-prefer-native-locators".into(),
                            message: format!(
                                "Attribute selector `{attr}...]` in `.locator()` — \
                                 use `{replacement}()` instead."
                            ),
                            severity: Severity::Warning,
                        });
                        break; // One diagnostic per locator call.
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
    fn flags_role_attribute_selector() {
        let diags = run(
            "login.test.ts",
            r#"page.locator('[role="button"]');"#,
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "playwright-prefer-native-locators");
        assert!(diags[0].message.contains("getByRole"));
    }

    #[test]
    fn flags_placeholder_attribute() {
        let diags = run(
            "form.spec.ts",
            r#"page.locator('[placeholder="Email"]');"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("getByPlaceholder"));
    }

    #[test]
    fn flags_data_testid_attribute() {
        let diags = run(
            "card.test.ts",
            r#"page.locator('[data-testid="card"]');"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("getByTestId"));
    }

    #[test]
    fn flags_alt_attribute() {
        let diags = run(
            "image.test.ts",
            r#"page.locator('[alt="logo"]');"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("getByAltText"));
    }

    #[test]
    fn allows_get_by_role() {
        let diags = run("login.test.ts", "page.getByRole('button');");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_locator_without_attribute() {
        let diags = run("login.test.ts", "page.locator('button');");
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run(
            "helpers.ts",
            r#"page.locator('[role="button"]');"#,
        );
        assert!(diags.is_empty());
    }
}
