//! playwright-max-nested-describe — limit nesting depth of describe blocks.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-max-nested-describe",
    description: "Deeply nested `describe` blocks reduce readability.",
    remediation: "Flatten the describe hierarchy to at most 5 levels deep.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/max-nested-describe.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
