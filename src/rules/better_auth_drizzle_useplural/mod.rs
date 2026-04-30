mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-drizzle-useplural",
    description: "`drizzleAdapter` with a `users` table requires `usePlural: true`.",
    remediation: "Add `usePlural: true` to the `drizzleAdapter(db, { ... })` options.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/adapters/drizzle"),
    categories: &["better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
