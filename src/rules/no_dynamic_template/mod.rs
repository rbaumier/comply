//! no-dynamic-template

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-dynamic-template",
    description: "Dynamic HTML construction via innerHTML, document.write, or similar APIs is an XSS vector.",
    remediation: "Use safe DOM APIs (`textContent`, `createElement`) or a framework's built-in escaping. Avoid raw HTML injection entirely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}
