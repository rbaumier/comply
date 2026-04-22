//! audit-log-required-fields

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "audit-log-required-fields",
    description: "Audit-log entries must carry enough context to reconstruct the event.",
    remediation: "Include `userId`, `timestamp`, and `action` (or equivalents) in every audit log call.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
