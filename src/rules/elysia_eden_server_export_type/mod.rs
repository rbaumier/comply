//! elysia-eden-server-export-type

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-eden-server-export-type",
    description: "Server entry file declares `new Elysia().listen(...)` but does not `export type` for Eden Treaty.",
    remediation: "Add `export type App = typeof app;` so the Eden Treaty client can infer routes from the server type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
