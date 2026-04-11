//! regex-anchor-precedence

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, ALL_TEXT_LANGUAGES};

pub const META: RuleMeta = RuleMeta {
    id: "regex-anchor-precedence",
    description: "Anchor `^` or `$` in alternation may not bind as expected.",
    remediation: "Wrap the alternation in a group: `/^(a|b)$/` instead of `/^a|b$/`. Without grouping, `/^a|b$/` means `(^a)|(b$)`, not `^(a|b)$`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
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
