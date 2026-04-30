//! detect-dangerous-redirects

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "detect-dangerous-redirects",
    description: "Redirecting to a user-controlled URL enables open-redirect attacks.",
    remediation: "Validate the redirect target against an allowlist of known-safe paths or origins before calling `res.redirect(...)`. Never pass `req.query.*`, `req.body.*`, or `req.params.*` straight through.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
