//! playwright-no-wait-for-selector — disallow `page.waitForSelector()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-wait-for-selector",
    description: "`page.waitForSelector()` is discouraged — use web-first assertions.",
    remediation: "Replace `waitForSelector` with a locator-based assertion \
                  like `await expect(page.locator(…)).toBeVisible()`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-wait-for-selector.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
