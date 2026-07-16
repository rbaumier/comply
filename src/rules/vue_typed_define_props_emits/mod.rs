//! vue-typed-define-props-emits — require type form in `<script setup lang="ts">`.
//!
//! Flags a runtime `defineProps({ ... })` / `defineEmits([...])` in a
//! `lang="ts"` setup block, except when the object argument composes a runtime
//! props object via a spread (`defineProps({ ...runtimeProps })`), which has no
//! type-only equivalent.

mod oxc_typescript;
mod oxc_vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-typed-define-props-emits",
    description: "In `lang=\"ts\"` SFCs, `defineProps({ ... })` / `defineEmits([...])` lose type inference.",
    remediation: "Use the type form: `defineProps<{ ... }>()` / `defineEmits<{ (e: 'x'): void }>()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(oxc_vue::Check)))],
    }
}
