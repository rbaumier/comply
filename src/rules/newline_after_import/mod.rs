//! newline-after-import

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "newline-after-import",
    description: "Missing blank line after the last import statement.",
    remediation: "Add an empty line between the last import and the first code statement for visual separation.",
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
