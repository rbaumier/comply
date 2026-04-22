//! no-path-traversal

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-path-traversal",
    description: "Using user-controlled input in `fs` calls without sanitization allows path traversal.",
    remediation: "Use `path.basename()` or validate against a safe root directory before any `fs` call.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
