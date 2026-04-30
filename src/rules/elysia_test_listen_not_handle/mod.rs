//! elysia-test-listen-not-handle

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-test-listen-not-handle",
    description: "Elysia test boots a real server with `.listen()` and uses `fetch()` instead of `app.handle(new Request(...))`.",
    remediation: "Drive the app in tests with `app.handle(new Request(...))` — no port binding, faster, deterministic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
