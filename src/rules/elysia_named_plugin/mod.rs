//! elysia-named-plugin

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-named-plugin",
    description: "Exported Elysia plugin instance has no `name` — deduplication and tracing degrade.",
    remediation: "Pass `new Elysia({ name: 'plugin-name' })` for plugins. Named plugins are deduplicated and surface in error traces.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
