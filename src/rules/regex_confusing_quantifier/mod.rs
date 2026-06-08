//! regex-confusing-quantifier

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-confusing-quantifier",
    description: "Quantifier is confusing because its minimum is non-zero but the quantified element can match the empty string.",
    remediation: "Replace the quantifier to reflect that it can match the empty string, e.g. use `*` instead of `+`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/confusing-quantifier.html",
    ),
    categories: &["regex"],

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
