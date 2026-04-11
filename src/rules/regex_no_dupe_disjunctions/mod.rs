//! regex-no-dupe-disjunctions

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, ALL_TEXT_LANGUAGES};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-dupe-disjunctions",
    description: "Regex contains duplicate alternatives that are redundant.",
    remediation: "Remove the duplicate alternative from the disjunction.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-dupe-disjunctions.html"),
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
