//! ts-assertion-fn-must-be-declaration — assertion functions cannot be arrows.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-assertion-fn-must-be-declaration",
    description: "Assertion functions (`asserts x`) cannot be arrow functions — TypeScript requires a function declaration or method.",
    remediation: "Rewrite the arrow as a `function` declaration: `function assertX(...): asserts x is T { ... }`.",
    severity: Severity::Error,
    doc_url: None,
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
