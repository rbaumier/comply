//! vue-no-duplicate-v-if — use `v-if/v-else`, not two opposite `v-if`s.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-duplicate-v-if",
    description: "Two opposite `v-if` conditions should be `v-if`/`v-else`.",
    remediation: "Replace `v-if=\"x\"` + `v-if=\"!x\"` with `v-if=\"x\"` / `v-else`. \
                  Two separate `v-if` directives evaluate independently — if the \
                  condition changes between the two evaluations, both render or \
                  neither does.",
    severity: Severity::Warning,
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
