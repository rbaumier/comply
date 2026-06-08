//! package-json-sorted-deps

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "package-json-sorted-deps",
    description: "Unsorted dependencies in package.json cause needless merge conflicts.",
    remediation: "Sort dependency keys alphabetically in each section \
                  (dependencies, devDependencies, peerDependencies).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["package-json"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::JavaScript, Backend::Text(Box::new(text::Check)))],
    }
}
