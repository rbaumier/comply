//! use-lone-executable-definition

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "use-lone-executable-definition",
    description: "A GraphQL document that defines more than one executable definition (operation or fragment) is harder to maintain, test, and reference; each should live in its own document.",
    remediation: "Move every executable definition after the first into its own document so each query, mutation, subscription, or fragment is defined alone.",
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
