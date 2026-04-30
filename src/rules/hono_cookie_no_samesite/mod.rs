//! hono-cookie-no-samesite

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-samesite",
    description: "Cookie without `sameSite` or with `sameSite: 'None'` — vulnerable to CSRF.",
    remediation: "Set `sameSite: 'Lax'` (default for most cases) or `sameSite: 'Strict'` for sensitive cookies.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
