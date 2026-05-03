//! prefer-called-with — prefer `toHaveBeenCalledWith(...)` over bare `toHaveBeenCalled()`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-called-with",
    description: "Prefer `toHaveBeenCalledWith(...)` over bare `toHaveBeenCalled()` to assert specific arguments.",
    remediation: "Use toHaveBeenCalledWith() to assert specific arguments",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jest-community/eslint-plugin-jest/blob/main/docs/rules/prefer-called-with.md",
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
