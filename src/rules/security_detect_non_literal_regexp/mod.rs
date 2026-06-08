//! security-detect-non-literal-regexp — `new RegExp(<variable>)`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-detect-non-literal-regexp",
    description: "`new RegExp(<dynamic>)` lets user input drive a regex — ReDoS / regex injection risk.",
    remediation: "Compile the regex from a static literal, or escape the user input first (no built-in helper — `s.replace(/[.*+?^${}()|[\\]\\\\]/g, '\\\\$&')`).",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-security/blob/main/docs/rules/detect-non-literal-regexp.md"),
    categories: &["security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
