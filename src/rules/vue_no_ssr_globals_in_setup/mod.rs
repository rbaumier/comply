//! vue-no-ssr-globals-in-setup — no `window`/`document`/etc at the top of `<script setup>`.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-ssr-globals-in-setup",
    description: "`window`, `document`, `localStorage`, `navigator` at the top of `<script setup>` crashes during SSR.",
    remediation: "Move the access into `onMounted(() => { ... })` — SSR renders `<script setup>` but not lifecycle hooks.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
