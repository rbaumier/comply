//! elysia-route-missing-params-schema

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-route-missing-params-schema",
    description: "Elysia route declares URL parameters but no `params:` schema.",
    remediation: "Add `params: t.Object({ id: t.Numeric(), ... })` so path params are validated and typed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["validation", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
