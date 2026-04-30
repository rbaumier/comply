//! playwright-no-commented-out-tests — flag commented-out test blocks.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-commented-out-tests",
    description: "Commented-out tests are dead code that hides missing coverage.",
    remediation: "Remove the commented-out test or re-enable it. Use `.skip()` \
                  if you need to temporarily disable it.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-commented-out-tests.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
