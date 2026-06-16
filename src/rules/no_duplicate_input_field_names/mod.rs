//! no-duplicate-input-field-names

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-input-field-names",
    description: "A GraphQL input object value that names the same field more than once is invalid; only the last occurrence of a repeated field is kept, silently dropping the earlier ones.",
    remediation: "Make every field of the input object value uniquely named, or remove the duplicate entries.",
    severity: Severity::Error,
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
