//! no-deprecated-api

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-deprecated-api",
    description: "Usage of deprecated Node.js or browser API.",
    remediation: "Replace with the modern equivalent: `Buffer.from()` instead of `new Buffer()`, `url.URL` instead of `url.parse()`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
