//! no-mass-assignment

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-mass-assignment",
    description: "Spreading `req.body` directly into a DB operation risks privilege escalation.",
    remediation: "Explicitly pick the fields you need from `req.body` instead of spreading it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
