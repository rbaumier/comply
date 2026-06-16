//! use-input-name

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "use-input-name",
    description: "A mutation field whose argument is not named `input` breaks the GraphQL convention of a single `input` argument, making the schema inconsistent and harder to evolve.",
    remediation: "Name every argument of a `Mutation` field `input` (typically a single `input` argument wrapping the mutation's parameters).",
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
