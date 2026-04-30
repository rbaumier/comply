//! hono-secure-headers-disabled

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-secure-headers-disabled",
    description: "Security header explicitly disabled in `secureHeaders()`.",
    remediation: "Don't disable security headers. Each one protects against a specific attack vector (HSTS, clickjacking, MIME sniffing, fingerprinting).",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
