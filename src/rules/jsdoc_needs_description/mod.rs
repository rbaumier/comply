//! jsdoc-needs-description

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-needs-description",
    description: "JSDoc block has tags but no description.",
    remediation: "Add a prose description to the JSDoc block. Tags alone don't explain what the function does or why.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
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
