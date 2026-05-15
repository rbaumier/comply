//! vitest-no-identical-title — duplicate `test()` / `it()` titles silently mask tests.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vitest-no-identical-title",
    description: "Two `test()` / `it()` blocks with the same title inside the same describe \
                  scope: vitest runs them both, but the test reporter merges them so only one \
                  result is visible.",
    remediation: "Make every test title unique within its describe scope, even if you have to \
                  add a discriminator like `\"… with empty input\"`.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/veritem/eslint-plugin-vitest/blob/main/docs/rules/no-identical-title.md"),
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
