//! playwright-no-eval — disallow `page.$eval()` / `page.$$eval()`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-eval",
    description: "`$eval` / `$$eval` evaluate arbitrary code against the DOM — brittle and hard to debug.",
    remediation: "Use `page.locator(...)` with web-first assertions like `toHaveText` / `toHaveAttribute` instead.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-eval.md",
    ),
    categories: &["testing"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
