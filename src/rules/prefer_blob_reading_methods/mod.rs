//! prefer-blob-reading-methods

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-blob-reading-methods",
    description: "Prefer `Blob#text()` / `Blob#arrayBuffer()` over `FileReader` methods.",
    remediation: "Use `await blob.text()` instead of `reader.readAsText(blob)`, or `await blob.arrayBuffer()` instead of `reader.readAsArrayBuffer(blob)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
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
