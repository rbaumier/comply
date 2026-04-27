//! elysia-ws-message-no-schema

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-ws-message-no-schema",
    description: "`.ws(...)` declares a `body:` schema but no `message:` — incoming WebSocket frames go unvalidated.",
    remediation: "Add a TypeBox `message:` schema describing each frame the server should accept.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
