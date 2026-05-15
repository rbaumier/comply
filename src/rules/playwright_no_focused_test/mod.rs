//! playwright-no-focused-test — alias of vitest-no-focused-tests for Playwright.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

mod oxc_typescript;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-focused-test",
    description: "`.only` on Playwright `test` / `test.describe` skips the rest of the suite.",
    remediation: "Remove `.only` before committing. Use Playwright's `--grep` flag to isolate a test temporarily.",
    severity: Severity::Error,
    doc_url: Some("https://playwright.dev/docs/api/class-test#test-only"),
    categories: &["testing", "playwright"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
