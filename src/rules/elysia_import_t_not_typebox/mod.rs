//! elysia-import-t-not-typebox

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-import-t-not-typebox",
    description: "TypeBox imported directly in an Elysia file — Elysia's `t` is the augmented public surface.",
    remediation: "Import `t` from `elysia` instead of `Type` from `@sinclair/typebox` — Elysia's `t` includes augmented validators.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["type-safety", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
