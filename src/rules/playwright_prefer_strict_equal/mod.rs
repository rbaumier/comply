//! playwright-prefer-strict-equal — suggest `toStrictEqual()` over `toEqual()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-strict-equal",
    description: "Prefer `toStrictEqual()` for more predictable deep equality checks.",
    remediation: "Replace `toEqual()` with `toStrictEqual()`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-strict-equal.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
