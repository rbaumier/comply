//! vue-no-setup-props-reactivity-loss — destructuring `defineProps` loses reactivity.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-setup-props-reactivity-loss",
    description: "Destructuring `defineProps()` in `<script setup>` strips reactivity from the props.",
    remediation: "Keep the props object intact: `const props = defineProps<...>()` and read \
                  `props.foo`. Reactive destructure requires the Vue 3.5+ reactive-props transform.",
    severity: Severity::Error,
    doc_url: Some("https://eslint.vuejs.org/rules/no-setup-props-reactivity-loss.html"),
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
