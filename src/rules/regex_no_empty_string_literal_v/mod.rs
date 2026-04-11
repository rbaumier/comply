//! regex-no-empty-string-literal-v

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, ALL_TEXT_LANGUAGES};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-string-literal-v",
    description: "Empty string disjunction in a `v`-flag character class is unexpected and likely a mistake.",
    remediation: "Remove the empty string literal from the character class string disjunction.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-empty-string-literal.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: ALL_TEXT_LANGUAGES
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
