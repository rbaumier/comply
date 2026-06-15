//! graphql-use-lone-anonymous-operation

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "graphql-use-lone-anonymous-operation",
    description: "An anonymous operation is only valid when it is the document's single operation; alongside other operations it cannot be referenced or distinguished.",
    remediation: "Give the anonymous operation a name (`query GetUser { ... }`), or split it into its own document so it stays the only operation defined there.",
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
