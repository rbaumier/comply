//! graphql-require-id-field

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "graphql-require-id-field",
    description: "Selecting object fields without `id` breaks normalized client caches.",
    remediation: "Add `id` to every multi-field object selection. Apollo and Relay key entities by `id` — without it, repeated queries return stale data.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["graphql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::GraphQl, Backend::Text(Box::new(text::Check)))],
    }
}
