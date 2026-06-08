//! regex-no-super-linear-move

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-super-linear-move",
    description: "Regex quantifier can cause quadratic runtime on certain inputs.",
    remediation: "Refactor the quantifier to avoid super-linear backtracking. Use atomic groups or restructure the pattern.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-super-linear-move.html",
    ),
    categories: &["security", "regex"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
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
