//! drizzle-prepared-placeholder

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-prepared-placeholder",
    description: "`prepare()` chains must use `sql.placeholder(...)` in `where`, not inline variables.",
    remediation: "Replace inline variables inside `.where(...)` of a `.prepare()` chain with `sql.placeholder('name')` and bind values at execution time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
