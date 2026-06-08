//! playwright-require-top-level-describe — tests must be inside `test.describe(...)`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-require-top-level-describe",
    description: "Bare `test(...)` at module top makes reports harder to scan — wrap related tests in `test.describe(...)`.",
    remediation: "Group tests in a `test.describe(\"<feature>\", () => { ... })` block. Each `test()` lives inside one describe.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/require-top-level-describe.md"),
    categories: &["testing", "playwright"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
