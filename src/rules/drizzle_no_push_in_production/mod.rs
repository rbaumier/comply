//! drizzle-no-push-in-production — `drizzle-kit push` bypasses migrations.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-push-in-production",
    description: "`drizzle-kit push` is for dev only — use migrations in production/CI.",
    remediation: "Replace `drizzle-kit push` with `drizzle-kit generate` + `drizzle-kit migrate` in CI/deployment scripts.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["drizzle", "database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
