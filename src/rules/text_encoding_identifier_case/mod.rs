//! text-encoding-identifier-case

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "text-encoding-identifier-case",
    description: "Enforce consistent case for text encoding identifiers (`utf-8`, `ascii`).",
    remediation: "Use lowercase: `'utf-8'` instead of `'UTF-8'`, `'ascii'` instead of `'ASCII'`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
