//! vue-setup-store-return-all — Pinia setup stores must return every ref/computed.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-setup-store-return-all",
    description: "A Pinia `defineStore('x', () => { ... })` must return every reactive state and computed.",
    remediation: "Return every `ref`, `reactive`, and `computed` declared in the setup — otherwise they are unusable outside.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
