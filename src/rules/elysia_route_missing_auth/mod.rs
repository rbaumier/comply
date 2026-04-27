//! elysia-route-missing-auth

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-route-missing-auth",
    description: "Sensitive routes (e.g. `/admin`, `/me`, `/profile`) lack an auth guard.",
    remediation: "Add a `beforeHandle` auth check or wrap the route in `.guard({ auth: ... })` before serving sensitive paths.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
