//! ts-only-throw-error — disallow throwing non-Error values.

mod oxc_typescript;


use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-only-throw-error",
    description: "Only `Error` instances should be thrown — primitives and plain objects lose stack traces.",
    remediation: "Throw `new Error(...)` (or a subclass) rather than a string, number, or object literal.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/only-throw-error/"),
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
