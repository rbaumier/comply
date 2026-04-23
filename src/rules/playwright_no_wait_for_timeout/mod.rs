//! playwright-no-wait-for-timeout — disallow `page.waitForTimeout()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-wait-for-timeout",
    description: "`page.waitForTimeout()` introduces fragile fixed sleeps in tests.",
    remediation: "Use web-first assertions or waitFor* with conditions instead of arbitrary timeouts",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-wait-for-timeout.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
