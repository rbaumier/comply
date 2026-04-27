//! drizzle-no-drizzle-kit-in-runtime

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-drizzle-kit-in-runtime",
    description: "`drizzle-kit` is a CLI/dev-time package — importing it from runtime code pulls migration tooling into the production bundle.",
    remediation: "Keep `drizzle-kit` imports inside `drizzle.config.ts` or migration scripts; runtime code should depend only on `drizzle-orm`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle", "bundle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
