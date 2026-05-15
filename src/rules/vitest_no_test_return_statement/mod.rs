//! vitest-no-test-return-statement — `return` in test callback masks async bugs.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-no-test-return-statement",
    description: "Returning a non-Promise value from a test callback is silently discarded.",
    remediation: "Drop the `return`. If you need a Promise, mark the callback `async` and `await` the value.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/veritem/eslint-plugin-vitest/blob/main/docs/rules/no-test-return-statement.md"),
    categories: &["testing", "vitest"],
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
