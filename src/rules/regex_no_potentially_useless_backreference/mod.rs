//! regex-no-potentially-useless-backreference

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-potentially-useless-backreference",
    description: "Backreference may be useless because some paths to it do not go through the referenced group.",
    remediation: "Restructure the regex so all paths to the backreference pass through the referenced capturing group.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-potentially-useless-backreference.html",
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
