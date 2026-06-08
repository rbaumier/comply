//! vue-no-filter-sort-in-template — no `.filter()`/`.sort()`/call expressions in `v-for`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-filter-sort-in-template",
    description: "`v-for` over `.filter()`/`.sort()` re-runs on every render.",
    remediation: "Extract to a `computed()` so the derived list is cached until its inputs change.",
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
