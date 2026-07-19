//! vue-v-memo-requires-v-for — standalone `v-memo="[]"` is redundant with `v-once`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-v-memo-requires-v-for",
    description: "`v-memo=\"[]\"` without `v-for` never re-renders — that's exactly what `v-once` states directly.",
    remediation: "Replace standalone `v-memo=\"[]\"` with `v-once`, or give `v-memo` a real dependency array.",
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
