//! playwright-no-duplicate-hooks — disallow duplicate setup/teardown hooks.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-duplicate-hooks",
    description: "Duplicate hooks in a describe block are confusing and error-prone.",
    remediation: "Merge the duplicate hooks into a single hook call.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-duplicate-hooks.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
