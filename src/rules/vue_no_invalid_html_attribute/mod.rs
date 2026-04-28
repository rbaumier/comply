//! vue-no-invalid-html-attribute

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-invalid-html-attribute",
    description: "Invalid value in HTML `rel` attribute.",
    remediation: "Use a valid `rel` value (`noopener`, `noreferrer`, `stylesheet`, etc.).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue", "html"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
