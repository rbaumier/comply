//! playwright-prefer-hooks-on-top — hooks should come before test cases.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-hooks-on-top",
    description: "Hooks should come before any test cases.",
    remediation: "Move hooks above the first `test()` / `it()` call.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-hooks-on-top.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
