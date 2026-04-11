//! playwright-prefer-to-be — suggest `toBe()` for primitive literals.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-to-be",
    description: "Use `toBe()` for primitives — `toEqual` does unnecessary deep comparison.",
    remediation: "Replace `toEqual(primitive)` with `toBe(primitive)`. \
                  Use `toBeNull()`, `toBeUndefined()`, `toBeNaN()`, `toBeDefined()` \
                  for their respective values.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-to-be.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
