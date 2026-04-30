//! playwright-no-conditional-in-test — disallow conditional logic inside test bodies.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-conditional-in-test",
    description: "Conditional logic in tests makes them non-deterministic.",
    remediation: "Remove `if`/`switch`/ternary from the test body. Write \
                  separate tests for each branch.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-conditional-in-test.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
