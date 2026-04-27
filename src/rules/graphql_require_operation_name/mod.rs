//! graphql-require-operation-name

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "graphql-require-operation-name",
    description: "Anonymous GraphQL operations are hard to identify in logs and tooling.",
    remediation: "Name every operation: `query GetUser { ... }` instead of `query { ... }`. Operation names appear in tracing, persisted-query keys, and devtools.",
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
