//! no-document-write

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-document-write",
    description: "Do not call `document.write()` / `document.writeln()`.",
    remediation: "Replace `document.write` with DOM APIs (`appendChild`, `innerHTML` with sanitization, or a framework). `document.write` re-opens the document after load and is an XSS vector.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
