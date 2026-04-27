//! elysia-apollo-playground-prod

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-apollo-playground-prod",
    description: "Apollo Playground enabled unconditionally — exposing the schema explorer in production leaks introspection.",
    remediation: "Gate `enablePlayground` on `process.env.NODE_ENV !== 'production'` (or another env flag).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
