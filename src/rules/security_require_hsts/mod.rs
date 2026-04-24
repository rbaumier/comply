//! security-require-hsts

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "security-require-hsts",
    description: "Express/HTTP apps must send a `Strict-Transport-Security` header.",
    remediation: "Install `helmet()` (enables HSTS by default) or set `Strict-Transport-Security` explicitly via `res.setHeader`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
