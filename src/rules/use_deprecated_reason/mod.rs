//! use-deprecated-reason

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "use-deprecated-reason",
    description: "A `@deprecated` directive must carry a non-empty `reason` argument so consumers know why the schema member is deprecated and what to use instead.",
    remediation: "Add a `reason` argument with an explanatory string to the `@deprecated` directive (e.g. `@deprecated(reason: \"Use `members` instead\")`).",
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
