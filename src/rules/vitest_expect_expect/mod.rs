//! vitest-expect-expect — test with no expect() call.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-expect-expect",
    description: "Test without any `expect()` call passes silently — the suite gives false confidence.",
    remediation: "Add at least one `expect(...)` assertion. If the test really only verifies that code doesn't throw, assert that explicitly (e.g. `expect(() => fn()).not.toThrow()`).",
    severity: Severity::Error,
    doc_url: Some("https://github.com/veritem/eslint-plugin-vitest/blob/main/docs/rules/expect-expect.md"),
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
