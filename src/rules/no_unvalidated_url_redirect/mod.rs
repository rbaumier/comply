//! no-unvalidated-url-redirect

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unvalidated-url-redirect",
    description: "Client-side redirect from user input creates an open redirect.",
    remediation: "Validate the redirect URL against an allowlist before assigning to `location`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
