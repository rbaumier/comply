//! graphql-no-duplicate-field-definition-names

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "graphql-no-duplicate-field-definition-names",
    description: "A type, interface, or input definition that declares the same field name twice is invalid — only one declaration takes effect.",
    remediation: "Remove the repeated field declaration so every field name in the `type`/`interface`/`input` definition (or its `extend` block) is unique.",
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
