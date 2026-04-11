//! playwright-expect-expect — enforce assertions in test bodies.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-expect-expect",
    description: "Test has no assertions — every test should verify behaviour.",
    remediation: "Add at least one `expect()` call inside the test body.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/expect-expect.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
