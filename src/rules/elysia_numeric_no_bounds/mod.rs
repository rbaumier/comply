//! elysia-numeric-no-bounds

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-numeric-no-bounds",
    description: "`t.Number()` / `t.Numeric()` is declared without `minimum` or `maximum` bounds.",
    remediation: "Add at least `{ minimum: 1 }` (IDs) or `{ minimum: 0, maximum: 100 }` (percentages) so the schema rejects out-of-range values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["validation", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
