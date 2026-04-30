//! elysia-cookie-no-samesite

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cookie-no-samesite",
    description: "Cookie config is missing an explicit `sameSite` — defaults are inconsistent across browsers.",
    remediation: "Set `sameSite: 'lax'` (or `'strict'` for sensitive cookies) explicitly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
