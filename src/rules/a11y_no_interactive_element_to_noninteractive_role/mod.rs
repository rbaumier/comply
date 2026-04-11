//! a11y-no-interactive-element-to-noninteractive-role

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-interactive-element-to-noninteractive-role",
    description: "Interactive elements must not be assigned non-interactive ARIA roles.",
    remediation: "Remove the non-interactive `role` or use a non-interactive element instead of `<button>`, `<a>`, `<input>`, `<select>`, or `<textarea>`.",
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
