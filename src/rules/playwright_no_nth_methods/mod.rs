//! playwright-no-nth-methods — disallow `.first()`, `.last()`, `.nth()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-nth-methods",
    description: "`.first()`, `.last()`, `.nth()` create fragile locators.",
    remediation: "Use a more specific locator (e.g. `getByRole`, `getByTestId`) \
                  instead of positional methods.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-nth-methods.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
