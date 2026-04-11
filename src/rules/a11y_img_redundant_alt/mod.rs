//! a11y-img-redundant-alt

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-img-redundant-alt",
    description: "`alt` text should not contain redundant words like \"image\", \"picture\", or \"photo\".",
    remediation: "Describe the image content instead of stating that it is an image. Remove words like \"image\", \"picture\", or \"photo\" from the `alt` attribute.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
