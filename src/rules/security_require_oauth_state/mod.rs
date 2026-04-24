//! security-require-oauth-state

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "security-require-oauth-state",
    description: "OAuth callback handlers must read and validate the `state` parameter.",
    remediation: "Read `state` from the callback request and compare it against the value stored before redirecting to the authorize URL.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
