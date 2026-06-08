//! vue-no-options-api — enforce Composition API with `<script setup>`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-options-api",
    description: "Use Composition API (`<script setup>`), not Options API.",
    remediation: "Replace `export default { data(), methods, computed }` with \
                  `<script setup lang=\"ts\">` using `ref()`, `computed()`, \
                  and plain functions. Options API is legacy in Vue 3.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
