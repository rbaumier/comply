//! hono-cookie-no-secure

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-secure",
    description: "Cookie set without `secure` — sent over unencrypted HTTP.",
    remediation: "Add `secure: true` to cookie options so the cookie is only sent over HTTPS.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "hono"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
