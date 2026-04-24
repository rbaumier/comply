//! drizzle-pool-requires-timeouts

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-pool-requires-timeouts",
    description: "`new Pool()` must define both `idleTimeoutMillis` and `connectionTimeoutMillis`.",
    remediation: "Add `idleTimeoutMillis` and `connectionTimeoutMillis` to the `new Pool(...)` config so stuck connections don't leak and new ones fail fast.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
