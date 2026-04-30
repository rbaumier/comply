//! playwright-prefer-web-first-assertions — flag manual boolean assertions on locator methods.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-web-first-assertions",
    description: "`expect(await locator.isVisible()).toBe(true)` does not auto-retry — use web-first assertions.",
    remediation: "Replace `expect(await el.isVisible()).toBe(true)` with \
                  `await expect(el).toBeVisible()`. Web-first assertions \
                  auto-retry until the condition is met or the timeout \
                  expires, making tests more reliable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
