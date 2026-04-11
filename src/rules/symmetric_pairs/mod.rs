//! symmetric-pairs

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "symmetric-pairs",
    description: "Exported function has no symmetric counterpart (get/set, add/remove, open/close, start/stop, create/delete).",
    remediation: "Add the missing counterpart or remove the export if the pair is intentionally incomplete.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["naming"],
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
