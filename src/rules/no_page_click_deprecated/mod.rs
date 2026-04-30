//! no-page-click-deprecated — reject deprecated `page.click()` in Playwright.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-page-click-deprecated",
    description: "`page.click(selector)` is deprecated — use `page.locator(selector).click()`.",
    remediation: "Replace `page.click(selector)` with \
                  `page.locator(selector).click()`. The locator API \
                  auto-waits and auto-retries, and the direct page methods \
                  are deprecated in Playwright.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
