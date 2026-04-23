mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-require-rate-limit",
    description: "Better Auth config without `rateLimit` leaves auth endpoints unprotected.",
    remediation: "Add `rateLimit: { enabled: true }` to your `betterAuth({})` config.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/rate-limiting"),
    categories: &["security", "better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
