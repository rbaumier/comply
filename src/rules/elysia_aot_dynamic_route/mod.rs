//! elysia-aot-dynamic-route

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-aot-dynamic-route",
    description: "Route paths built via template literals or string concatenation defeat Elysia's AOT compilation.",
    remediation: "Pass a static string literal as the route path; bind dynamic segments with `:param`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
