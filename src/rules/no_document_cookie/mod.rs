//! no-document-cookie

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-document-cookie",
    description: "Do not use `document.cookie` directly.",
    remediation: "Use a cookie library (e.g. `js-cookie`, `cookie`) instead of raw `document.cookie` access. Direct cookie manipulation is error-prone and hard to maintain.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
