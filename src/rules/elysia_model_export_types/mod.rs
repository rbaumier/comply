//! elysia-model-export-types

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-model-export-types",
    description: "Module exports a `t.Object(...)` const but no static type derived from it.",
    remediation: "Export the inferred type alongside the schema: `export type User = typeof UserModel.static;`",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["type-safety", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
