//! vue-no-setup-props-reactivity-loss — destructuring `defineProps` loses reactivity.
//!
//! Warns about destructured `defineProps()` in `<script setup>` ONLY when the
//! project's nearest `package.json` declares a Vue version below 3.5. Reactive
//! Props Destructuring is stable since Vue 3.5: the SFC compiler rewrites every
//! destructured prop reference back to `props.x`, so reactivity is preserved and
//! the pattern is safe. When the declared Vue version is >= 3.5 the rule is a
//! no-op; with no declared Vue version it keeps warning.

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
