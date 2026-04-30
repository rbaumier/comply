//! elysia-route-all-method

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-route-all-method",
    description: "`.all()` accepts any HTTP method — usually a specific method is more appropriate.",
    remediation: "Replace `.all('/path', ...)` with `.get`, `.post`, `.put`, `.patch`, or `.delete` to communicate intent and let routers/proxies cache safely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
