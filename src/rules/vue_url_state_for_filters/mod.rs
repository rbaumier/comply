//! vue-url-state-for-filters — filter/pagination state should live in the URL.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vue-url-state-for-filters",
    description: "Store filter/pagination state in the URL, not in local `ref()`.",
    remediation: "Filters, pagination, search, and sort state should survive a page \
                  reload and be shareable by URL. Use `useUrlSearchParams` from \
                  VueUse (or your router's query) instead of a local `ref()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
