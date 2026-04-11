//! a11y-no-noninteractive-element-to-interactive-role

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-noninteractive-element-to-interactive-role",
    description: "Non-interactive elements must not be assigned interactive ARIA roles.",
    remediation: "Use a native interactive element (`<button>`, `<a>`) instead of adding an interactive `role` to a `<div>`, `<span>`, etc.",
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
