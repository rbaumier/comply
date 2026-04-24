//! drizzle-config-satisfies

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-config-satisfies",
    description: "`drizzle.config.ts` should use `satisfies Config` instead of `: Config` annotations.",
    remediation: "Replace `const config: Config = { ... }` with `export default { ... } satisfies Config` so Drizzle kit narrows the config type without widening.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
