//! vue-watch-immediate-over-onmounted — prefer `watch(..., { immediate: true })`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-watch-immediate-over-onmounted",
    description: "A `watch` paired with an `onMounted` that runs the same callback duplicates logic.",
    remediation: "Drop the `onMounted` and pass `{ immediate: true }` to the watch.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
