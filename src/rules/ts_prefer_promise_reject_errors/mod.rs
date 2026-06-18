//! ts-prefer-promise-reject-errors — require `Promise.reject()` to be called with an `Error`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-promise-reject-errors",
    description: "`Promise.reject()` should receive an `Error` instance, not a primitive or plain object.",
    remediation: "Call `Promise.reject(new Error('...'))` instead of passing a string, number, or object literal.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-promise-reject-errors/"),
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
