//! playwright-prefer-comparison-matcher — suggest built-in comparison matchers.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-comparison-matcher",
    description: "Use built-in comparison matchers instead of comparing manually.",
    remediation: "Replace `expect(a > b).toBe(true)` with \
                  `expect(a).toBeGreaterThan(b)`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-comparison-matcher.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
