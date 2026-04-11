//! playwright-no-hooks — disallow setup and teardown hooks.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-hooks",
    description: "Hooks add implicit shared state between tests.",
    remediation: "Replace hooks with explicit helper functions called in each \
                  test body.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-hooks.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
