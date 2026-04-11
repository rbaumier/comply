//! playwright-no-useless-await — flag unnecessary `await` on sync Playwright methods.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-useless-await",
    description: "Unnecessary `await` on synchronous Playwright methods.",
    remediation: "Remove the `await` — this method does not return a Promise.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-useless-await.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
