//! elysia-cookie-getter-setter

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-cookie-getter-setter",
    description: "Code uses `cookie.get(...)` / `cookie.set(...)` — Elysia exposes cookies as `cookie.<name>.value`.",
    remediation: "Use `ctx.cookie.<name>.value` for reads and assignment for writes (e.g. `cookie.session.value = '...'`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
