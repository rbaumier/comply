//! regex-no-invisible-character

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, ALL_TEXT_LANGUAGES};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-invisible-character",
    description: "Invisible Unicode characters in regex (zero-width joiners, soft hyphens, etc.) are hard to spot and usually unintended.",
    remediation: "Use explicit Unicode escapes (`\\u{200D}`) instead of embedding invisible characters directly.",
    severity: Severity::Warning,
    doc_url: None,
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
