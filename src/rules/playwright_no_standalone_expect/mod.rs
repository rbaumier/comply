//! playwright-no-standalone-expect — disallow `expect` outside test blocks.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-no-standalone-expect",
    description: "`expect()` outside a test body never runs as an assertion.",
    remediation: "Move the `expect()` call inside a `test()` or `it()` block.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/no-standalone-expect.md",
    ),
    categories: &["testing"],
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
