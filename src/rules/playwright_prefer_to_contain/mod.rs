//! playwright-prefer-to-contain — suggest `toContain` over `includes()` + equality.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-to-contain",
    description: "Use `toContain()` instead of `expect(arr.includes(x)).toBe(true)`.",
    remediation: "Replace `expect(arr.includes(x)).toBe(true)` with \
                  `expect(arr).toContain(x)`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-to-contain.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
