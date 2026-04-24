//! vue-no-usestore-top-level — no `useOtherStore()` at the top of a Pinia store setup.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-usestore-top-level",
    description: "Calling another `useXxxStore()` at the top of a store setup pins Pinia initialization order.",
    remediation: "Move the `useOtherStore()` call inside an action or getter so it resolves at use time.",
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
