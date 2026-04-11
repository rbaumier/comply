//! playwright-no-useless-not — disallow `not` when a direct matcher exists.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-useless-not",
    description: "Using `.not.toBeVisible()` when `.toBeHidden()` exists is needlessly indirect.",
    remediation: "Use the direct matcher instead of negating: \
                  `toBeHidden` instead of `not.toBeVisible`, \
                  `toBeDisabled` instead of `not.toBeEnabled`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-useless-not.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
