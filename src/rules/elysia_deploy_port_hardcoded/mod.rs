//! elysia-deploy-port-hardcoded

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-deploy-port-hardcoded",
    description: "Elysia `.listen()` uses a hardcoded numeric port — deployment platforms typically inject the port via environment.",
    remediation: "Read the port from `process.env.PORT` (with a sensible default) so the same image works locally and on hosting.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["deployment", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
