//! ts-prefer-function-type — prefer function types over interfaces with
//! only a call signature.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-function-type",
    description: "An interface with only a call signature should be a function type.",
    remediation: "Replace `interface Fn { (): T }` with `type Fn = () => T`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-function-type/"),
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
