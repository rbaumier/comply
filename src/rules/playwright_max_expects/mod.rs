//! playwright-max-expects — limit assertion count per test body.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-max-expects",
    description: "Too many assertions in a single test — split into focused tests.",
    remediation: "Keep each test to ≤ 5 `expect()` calls. Extract additional \
                  assertions into separate test cases.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/max-expects.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
