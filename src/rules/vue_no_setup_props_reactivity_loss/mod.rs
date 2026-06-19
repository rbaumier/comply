//! vue-no-setup-props-reactivity-loss — destructuring `defineProps` loses reactivity.
//!
//! Warns about destructured `defineProps()` in `<script setup>` ONLY when the
//! project does not provably ship Vue 3.5+. Reactive Props Destructuring is
//! stable since Vue 3.5: the SFC compiler rewrites every destructured prop
//! reference back to `props.x`, so reactivity is preserved and the pattern is
//! safe. The rule is a no-op when the nearest `package.json` declares either
//! `vue >= 3.5` or `nuxt >= 4` (Nuxt 4 ships Vue 3.5+ transitively, so its
//! projects often declare only `nuxt` with no direct `vue` dependency). With
//! neither declared it keeps warning.

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

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
