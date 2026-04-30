mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-no-disable-csrf",
    description: "`disableCSRFCheck: true` removes CSRF protection from Better Auth.",
    remediation: "Remove `disableCSRFCheck` — CSRF protection is enabled by default and must stay on.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/security"),
    categories: &["security", "better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
