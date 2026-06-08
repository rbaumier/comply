//! vue-prefer-computed — prefer `computed()` over `watch()` when the callback
//! just mirrors the watched source into another ref.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-prefer-computed",
    description: "Use `computed()` when a watcher only assigns a derived value to another ref.",
    remediation: "A `watch(src, () => { target.value = fn(src.value) })` pattern is a \
                  derived value — use `const target = computed(() => fn(src.value))` \
                  instead. `computed()` is lazy, cached, and cannot desync.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
