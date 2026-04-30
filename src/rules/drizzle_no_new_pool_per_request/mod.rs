//! drizzle-no-new-pool-per-request

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-new-pool-per-request",
    description: "`new Pool()` or `drizzle()` must be called at module scope, not inside a handler body.",
    remediation: "Move the `new Pool(...)` / `drizzle(...)` initialization to module scope so connections are reused across requests.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
