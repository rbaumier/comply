//! elysia-global-with-types

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-global-with-types",
    description: "Plugin uses `as: 'global'` while also exposing `state`, `decorate`, or `model` — global scope leaks types into every consumer.",
    remediation: "Use `as: 'scoped'` for plugins that publish typed context. Reserve `'global'` for hook-only plugins (logging, telemetry).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["type-safety", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
