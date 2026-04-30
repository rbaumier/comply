//! elysia-onerror-before-plugin

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-onerror-before-plugin",
    description: "`.onError(...)` registered after `.use(plugin)` does not catch errors thrown by that plugin.",
    remediation: "Chain `.onError(...)` before `.use(plugin)` so the handler is in scope when the plugin registers its routes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
