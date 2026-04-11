//! regex-no-obscure-range

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY_AND_RUST};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-obscure-range",
    description: "Character class ranges like `[A-z]` include unwanted chars (`[\\]^_\\``). Use `[A-Za-z]` instead.",
    remediation: "Replace obscure ranges with explicit ones: `[A-Za-z]`, `[a-zA-Z0-9]`, etc.",
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
