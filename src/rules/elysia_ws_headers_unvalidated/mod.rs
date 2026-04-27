//! elysia-ws-headers-unvalidated

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-ws-headers-unvalidated",
    description: "WebSocket route reads request headers in `beforeHandle` but does not declare a header schema.",
    remediation: "Add a `headers` (TypeBox) schema so Elysia validates the upgrade request headers before invoking `beforeHandle`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
