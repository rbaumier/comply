//! elysia-listen-callback-info

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-listen-callback-info",
    description: "`.listen(PORT)` called without a callback — server boot info is silently dropped.",
    remediation: "Pass a callback to `.listen` and log `app.server?.hostname`/`app.server?.port` so deploys surface where the server is actually bound.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["observability", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
