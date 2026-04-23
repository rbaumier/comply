//! playwright-no-eval — disallow `page.$eval()` / `page.$$eval()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-eval",
    description: "`$eval` / `$$eval` evaluate arbitrary code against the DOM — brittle and hard to debug.",
    remediation: "Use `page.locator(...)` with web-first assertions like `toHaveText` / `toHaveAttribute` instead.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-eval.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
