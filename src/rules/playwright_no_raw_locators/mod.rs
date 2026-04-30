//! playwright-no-raw-locators — flag `page.locator()` with CSS selectors.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-raw-locators",
    description: "`page.locator('css-selector')` is brittle — prefer `getByRole`, `getByText`, etc.",
    remediation: "Replace `page.locator('.btn')` with \
                  `page.getByRole('button')` or `page.getByText('Submit')`. \
                  Semantic locators are resilient to markup changes and \
                  align with how users find elements on the page.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
