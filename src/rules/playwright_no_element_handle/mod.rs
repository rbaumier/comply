//! playwright-no-element-handle — flag `page.$()` and `page.$$()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-element-handle",
    description: "`page.$()` / `page.$$()` return ElementHandles, which are deprecated in favor of Locators.",
    remediation: "Replace `page.$('selector')` with `page.locator('selector')` \
                  and `page.$$('selector')` with \
                  `page.locator('selector').all()`. Locators auto-wait and \
                  retry, while ElementHandles are stale references that \
                  break on re-renders.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
