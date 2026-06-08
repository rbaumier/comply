//! vue-define-model-over-modelvalue — prefer `defineModel` (Vue 3.4+).

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-define-model-over-modelvalue",
    description: "`defineProps<{ modelValue }>` + `update:modelValue` is superseded by `defineModel` in Vue 3.4+.",
    remediation: "Replace the `modelValue` prop and `update:modelValue` emit with `const model = defineModel()`.",
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
