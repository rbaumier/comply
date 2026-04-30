//! vue-no-mutate-prop — props are one-way; never mutate them in-place.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-mutate-prop",
    description: "Don't mutate a prop directly — props are one-way.",
    remediation: "Emit an event or copy the prop into a local ref to mutate.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
