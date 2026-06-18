//! ts-no-unnecessary-parameter-property-assignment — disallow redundant
//! `this.x = x` when `x` is already a parameter property.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unnecessary-parameter-property-assignment",
    description: "Assigning `this.x = x` in a constructor is redundant when `x` is already a parameter property.",
    remediation: "Remove the redundant assignment — the parameter property already handles it.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://typescript-eslint.io/rules/no-unnecessary-parameter-property-assignment/",
    ),
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
