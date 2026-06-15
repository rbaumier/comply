//! graphql-use-deprecated-date

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "graphql-use-deprecated-date",
    description: "A `@deprecated` directive must carry a `deletionDate` argument so deprecated schema members have a scheduled removal date.",
    remediation: "Add a `deletionDate: \"YYYY-MM-DD\"` argument to the `@deprecated` directive (e.g. `@deprecated(reason: \"…\", deletionDate: \"2099-12-25\")`). If the date has already passed, remove the deprecated member or move the date into the future.",
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
