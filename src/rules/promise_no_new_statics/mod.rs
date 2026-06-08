//! promise-no-new-statics — `new Promise.resolve(...)` etc. is a bug.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "promise-no-new-statics",
    description: "`new Promise.resolve(...)` / `new Promise.reject(...)` calls a static as a constructor and throws at runtime.",
    remediation: "Drop the `new`: `Promise.resolve(value)`, `Promise.reject(error)`, etc.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-promise/blob/main/docs/rules/no-new-statics.md"),
    categories: &["promise"],

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
