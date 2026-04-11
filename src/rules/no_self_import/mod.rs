//! no-self-import

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-self-import",
    description: "Module imports itself.",
    remediation: "Remove the self-import. A module should never import from itself — it causes circular dependency issues.",
    severity: Severity::Error,
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
