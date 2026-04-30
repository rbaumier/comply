//! elysia-booleanstring-for-body

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-booleanstring-for-body",
    description: "`t.Boolean()` inside a `body:` schema rejects `\"true\"` / `\"false\"` form fields.",
    remediation: "Use `t.BooleanString()` for form-encoded payloads where booleans arrive as strings.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["validation", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
