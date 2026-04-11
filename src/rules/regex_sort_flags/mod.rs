//! regex-sort-flags

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY_AND_RUST};

pub const META: RuleMeta = RuleMeta {
    id: "regex-sort-flags",
    description: "Regex flags should be alphabetically sorted for consistency (`dgimsvy`).",
    remediation: "Reorder the flags alphabetically: e.g. `/pattern/gi` → `/pattern/gi` is already sorted, but `/pattern/ig` → `/pattern/gi`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY_AND_RUST
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
