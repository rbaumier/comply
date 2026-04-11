//! group-exports

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "group-exports",
    description: "Multiple named export declarations — consolidate into a single export block.",
    remediation: "Gather all named exports into a single `export { … }` declaration at the bottom of the file instead of scattering `export` across multiple declarations.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
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
