//! elysia-no-mix-zod-typebox

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-no-mix-zod-typebox",
    description: "File mixes Zod and Elysia's TypeBox `t` for validation — pick one schema library.",
    remediation: "Standardize on Elysia's `t.Object(...)` for route validation. Zod schemas are not understood by Elysia's type inference.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["type-safety", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
