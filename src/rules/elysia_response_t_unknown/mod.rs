//! elysia-response-t-unknown

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-response-t-unknown",
    description: "`response: t.Unknown()` / `t.Any()` disables response validation, so Eden inherits no type-safety.",
    remediation: "Describe the response with a concrete TypeBox schema (`t.Object({...})`, `t.String()`, etc.).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
