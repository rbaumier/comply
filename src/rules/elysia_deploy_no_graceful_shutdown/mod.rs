//! elysia-deploy-no-graceful-shutdown

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-deploy-no-graceful-shutdown",
    description: "Elysia server `.listen()` without graceful shutdown — in-flight requests are dropped on SIGTERM/SIGINT.",
    remediation: "Register a `process.on('SIGTERM', ...)` (and SIGINT) handler that calls `app.stop()` to drain connections before exit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["deployment", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
