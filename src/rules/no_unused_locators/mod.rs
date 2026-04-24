mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-unused-locators",
    description: "Playwright locator declared but never used — no action or assertion is called on it.",
    remediation: "Either use the locator (call an action like `.click()`, `.fill()`, or an \
                  assertion like `expect(locator).toBeVisible()`), or remove the declaration.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-unused-locators.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
