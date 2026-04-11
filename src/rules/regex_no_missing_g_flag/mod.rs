//! regex-no-missing-g-flag

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, ALL_TEXT_LANGUAGES};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-missing-g-flag",
    description: "Regex used with a method that expects the global flag but the g flag is missing.",
    remediation: "Add the `g` flag to the regex or use a method that does not require it.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-missing-g-flag.html"),
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
