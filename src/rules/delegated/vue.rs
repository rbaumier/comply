//! eslint-plugin-vue rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_delegate};

pub fn register_all() -> Vec<RuleDef> {
    vec![oxlint_delegate(
        RuleMeta {
            id: "vue-no-import-compiler-macros",
            description: "Don't import Vue compiler macros — they are globally available.",
            remediation: "Remove the import of `defineProps`/`defineEmits`/`defineModel` (and \
                          other compiler macros) from `vue`. The Vue compiler injects them \
                          automatically inside `<script setup>`; importing them is redundant \
                          and breaks the macro transform.",
            severity: Severity::Warning,
            doc_url: None,
            categories: &["vue"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        "vue/no-import-compiler-macros",
        TS_FAMILY,
    )]
}
