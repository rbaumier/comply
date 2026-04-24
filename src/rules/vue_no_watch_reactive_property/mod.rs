//! vue-no-watch-reactive-property — flag `watch(state.prop, ...)` (value, not getter).

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-watch-reactive-property",
    description: "`watch(state.prop, ...)` passes a snapshot — the watcher fires once then never again.",
    remediation: "Use a getter: `watch(() => state.prop, ...)`, or destructure with `toRefs`.",
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
