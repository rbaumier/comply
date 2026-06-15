//! graphql-no-duplicate-fields

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "graphql-no-duplicate-fields",
    description: "Duplicated fields, arguments, or variables in a GraphQL operation are redundant or conflicting.",
    remediation: "Remove the repeated entry. A field's response key (its alias, or its name when unaliased) must be unique within a selection set; argument names and variable names must be unique within their list. Use distinct aliases when you need the same field twice: `a: field`, `b: field`.",
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
