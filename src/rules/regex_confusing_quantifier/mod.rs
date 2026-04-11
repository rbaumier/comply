//! regex-confusing-quantifier

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, ALL_TEXT_LANGUAGES};

pub const META: RuleMeta = RuleMeta {
    id: "regex-confusing-quantifier",
    description: "Quantifier is confusing because its minimum is non-zero but the quantified element can match the empty string.",
    remediation: "Replace the quantifier to reflect that it can match the empty string, e.g. use `*` instead of `+`.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/confusing-quantifier.html"),
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
