//! graphql-no-inline-arguments

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "graphql-no-inline-arguments",
    description: "Inline literal arguments defeat query batching and persisted queries.",
    remediation: "Promote inline literals to variables: `user(id: $id)` with `$id: ID!` declared at the operation. Variables make queries cacheable and safe from injection.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["graphql"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::GraphQl, Backend::Text(Box::new(text::Check)))],
    }
}
