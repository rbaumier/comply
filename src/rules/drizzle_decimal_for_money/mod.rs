//! drizzle-decimal-for-money

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-decimal-for-money",
    description: "`numeric('price')` / `decimal('amount')` for money columns must declare `precision`/`scale` — otherwise the underlying SQL type is unbounded.",
    remediation: "Pass `{ precision: ..., scale: ... }` (e.g. `numeric('price', { precision: 12, scale: 2 })`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "drizzle", "database"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
