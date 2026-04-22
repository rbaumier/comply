//! no-ssrf-fetch

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-ssrf-fetch",
    description: "Server-side `fetch()` / `axios` call whose URL comes from request data can be turned into SSRF.",
    remediation: "Validate the URL against a host allowlist (and reject internal IPs) before issuing the request.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
