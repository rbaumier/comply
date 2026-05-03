//! consistent-function-scoping

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "consistent-function-scoping",
    description: "Nested function does not capture any variable from its parent scope and could be hoisted.",
    remediation: "Move the inner function to the outer scope or module level. Functions that don't close over parent state belong at the top level where they're easier to test and reuse.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/sindresorhus/eslint-plugin-unicorn/blob/main/docs/rules/consistent-function-scoping.md",
    ),
    categories: &["unicorn"],
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
