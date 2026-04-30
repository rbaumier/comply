//! elysia-plugin-functional-callback

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-plugin-functional-callback",
    description: "Functional plugins `(app: Elysia) => app.get(...)` lose type inference — prefer `new Elysia()` instances.",
    remediation: "Export `new Elysia({ name }).get(...)` and `.use(plugin)` instead of an arrow that mutates the parent app.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
