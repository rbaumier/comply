//! vue-custom-directive-v-prefix — local directives in `<script setup>` must be named `vXxx`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-custom-directive-v-prefix",
    description: "Local directives declared in `<script setup>` must start with `v` + capital letter.",
    remediation: "Rename `focus` → `vFocus`; `<script setup>` treats `vFoo` bindings as directives.",
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
