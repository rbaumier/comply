//! security-require-rate-limit-auth

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-require-rate-limit-auth",
    description: "Auth routes (`/login`, `/signup`, `/reset`) must be rate-limited.",
    remediation: "Add a rate-limit middleware (e.g. `rateLimit`, `rateLimiter`) to the auth route handler chain.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
