//! elysia-ws-missing-auth

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-ws-missing-auth",
    description: "Elysia `.ws()` route declared without a `beforeHandle` guard.",
    remediation: "Add `beforeHandle` to authenticate the upgrade request before accepting the WebSocket connection.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
