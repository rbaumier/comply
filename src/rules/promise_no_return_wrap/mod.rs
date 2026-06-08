//! promise-no-return-wrap — `return Promise.resolve(x)` inside `.then()`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "promise-no-return-wrap",
    description: "Wrapping a value in `Promise.resolve(x)` / `Promise.reject(e)` inside `.then()` is redundant.",
    remediation: "Return the value directly. The `.then` chain wraps it automatically. For errors, `throw e` instead of `return Promise.reject(e)`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-promise/blob/main/docs/rules/no-return-wrap.md"),
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
