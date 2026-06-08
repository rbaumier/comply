//! vitest-no-standalone-expect — `expect()` outside any test block.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-no-standalone-expect",
    description: "`expect(...)` outside any `test` / `it` / `describe` block runs at import time, not as a test.",
    remediation: "Move the `expect` inside a `test(...)` / `it(...)` body. If it's setup verification, use `beforeAll(...)` instead.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/veritem/eslint-plugin-vitest/blob/main/docs/rules/no-standalone-expect.md"),
    categories: &["testing", "vitest"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
