//! html-no-duplicate-attrs — flag duplicate attributes on the same HTML element.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-duplicate-attrs",
    description: "HTML elements must not declare the same attribute twice.",
    remediation: "Remove duplicate attribute",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["html"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
