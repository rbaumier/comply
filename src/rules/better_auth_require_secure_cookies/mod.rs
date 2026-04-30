mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-require-secure-cookies",
    description: "Better Auth config missing `useSecureCookies: true` — session cookies transmitted over HTTP.",
    remediation: "Add `advanced: { useSecureCookies: true }` to your Better Auth config for production.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/concepts/cookies"),
    categories: &["security", "better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
