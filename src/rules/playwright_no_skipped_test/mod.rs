//! playwright-no-skipped-test — disallow `.skip()` test annotation.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-skipped-test",
    description: "Skipped tests silently erode coverage.",
    remediation: "Remove the `.skip()` annotation, fix the test, or delete it \
                  if it's no longer relevant.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-skipped-test.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
