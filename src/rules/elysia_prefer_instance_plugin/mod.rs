//! elysia-prefer-instance-plugin

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-prefer-instance-plugin",
    description: "Plugin defined as a `(app: Elysia) => app...` callback — Elysia instance plugins are preferred for type inference and deduplication.",
    remediation: "Define plugins as `new Elysia({ name: '...' })...` instances. Callback plugins lose deduplication and degrade type inference.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["type-safety", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
