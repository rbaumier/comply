//! vue-no-target-blank

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-target-blank",
    description: "Opening links in a new tab without `rel=noreferrer` is a security risk.",
    remediation: "Add `rel=noreferrer` when using `target=_blank`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue", "security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
