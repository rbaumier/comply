//! regex-no-invisible-character

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-invisible-character",
    description: "Invisible Unicode characters in regex (zero-width joiners, soft hyphens, etc.) are hard to spot and usually unintended.",
    remediation: "Use explicit Unicode escapes (`\\u{200D}`) instead of embedding invisible characters directly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
