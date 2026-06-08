//! ts-explicit-function-return-type — require explicit return types on functions.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-function-return-type",
    description: "Require explicit return types on functions and class methods.",
    remediation: "Add an explicit `: ReturnType` annotation after the parameter \
                  list. Explicit return types make function contracts visible \
                  and prevent silent drift when the implementation changes.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-function-return-type/"),
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
