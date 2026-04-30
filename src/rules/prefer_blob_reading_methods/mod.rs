//! prefer-blob-reading-methods

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-blob-reading-methods",
    description: "Prefer `Blob#text()` / `Blob#arrayBuffer()` over `FileReader` methods.",
    remediation: "Use `await blob.text()` instead of `reader.readAsText(blob)`, or `await blob.arrayBuffer()` instead of `reader.readAsArrayBuffer(blob)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
