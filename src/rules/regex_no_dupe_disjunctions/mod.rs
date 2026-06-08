//! regex-no-dupe-disjunctions

mod oxc_typescript;


use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-dupe-disjunctions",
    description: "Regex contains duplicate alternatives that are redundant.",
    remediation: "Remove the duplicate alternative from the disjunction.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-dupe-disjunctions.html",
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
