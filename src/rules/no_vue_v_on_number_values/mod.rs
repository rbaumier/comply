//! no-vue-v-on-number-values — disallow deprecated number modifiers on Vue `v-on` directives.
//!
//! Vue 2 allowed numeric `keyCode` modifiers on `v-on` key events
//! (`v-on:keyup.13`, `@keyup.27`). Vue 3 dropped this support, so a `v-on`
//! directive (longhand or its `@` shorthand) carrying an all-digit modifier is
//! reported.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-vue-v-on-number-values",
    description: "Number (`keyCode`) modifiers on Vue `v-on` directives are deprecated and removed in Vue 3.",
    remediation: "Replace the numeric modifier with a named key modifier (e.g. `@keyup.enter`) or check the key code inside the handler.",
    severity: Severity::Error,
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
