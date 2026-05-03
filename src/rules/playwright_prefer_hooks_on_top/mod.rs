//! playwright-prefer-hooks-on-top — hooks should come before test cases.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "playwright-prefer-hooks-on-top",
    description: "Hooks should come before any test cases.",
    remediation: "Move hooks above the first `test()` / `it()` call.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/playwright-community/eslint-plugin-playwright/blob/main/docs/rules/prefer-hooks-on-top.md",
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
