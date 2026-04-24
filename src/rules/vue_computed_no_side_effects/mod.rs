//! vue-computed-no-side-effects — forbid side effects inside `computed()`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-computed-no-side-effects",
    description: "`computed()` must be pure — no emits, logs, API calls, mutations, or assignments.",
    remediation: "Move side effects to a `watch`, an event handler, or an action. `computed` should only derive a value.",
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
