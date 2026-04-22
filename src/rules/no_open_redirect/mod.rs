//! no-open-redirect

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-open-redirect",
    description: "Redirecting to a URL from user input creates an open redirect vulnerability.",
    remediation: "Validate the redirect target against an allowlist of trusted paths before redirecting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
