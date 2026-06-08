//! regex-no-useless-dollar-replacements

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-dollar-replacements",
    description: "Replacement string references a capturing group that does not exist in the regex.",
    remediation: "Fix the replacement reference to match an existing capturing group, or use `$$` to insert a literal dollar sign.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-useless-dollar-replacements.html",
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
