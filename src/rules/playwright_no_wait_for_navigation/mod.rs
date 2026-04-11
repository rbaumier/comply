//! playwright-no-wait-for-navigation — disallow `page.waitForNavigation()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-wait-for-navigation",
    description: "`page.waitForNavigation()` is discouraged — use `waitForURL` instead.",
    remediation: "Replace `waitForNavigation()` with `page.waitForURL(url)` \
                  or a web-first assertion.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-wait-for-navigation.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
