//! elysia-array-no-bounds

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-array-no-bounds",
    description: "`t.Array(...)` is declared without `minItems` / `maxItems` — clients can DoS the API with huge payloads.",
    remediation: "Pass `{ minItems, maxItems }` as the second argument: `t.Array(t.String(), { maxItems: 100 })`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
