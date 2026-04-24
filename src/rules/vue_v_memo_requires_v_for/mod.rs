//! vue-v-memo-requires-v-for — `v-memo` must be on a `v-for` element (or `v-memo="[]"`).

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-v-memo-requires-v-for",
    description: "`v-memo` is meaningful on `v-for` loops; elsewhere it's a noise directive.",
    remediation: "Apply `v-memo` to the element with `v-for`, or use `v-memo=\"[]\"` on a static subtree.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
