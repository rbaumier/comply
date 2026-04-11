//! playwright-prefer-equality-matcher — suggest built-in equality matchers.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-equality-matcher",
    description: "Use an equality matcher instead of `expect(a === b).toBe(true)`.",
    remediation: "Replace with `expect(a).toBe(b)` or `expect(a).toEqual(b)`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-equality-matcher.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
