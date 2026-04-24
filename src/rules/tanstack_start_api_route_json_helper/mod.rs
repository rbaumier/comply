//! tanstack-start-api-route-json-helper — API route handlers must use
//! `json()` from `@tanstack/react-start`, not `new Response(JSON.stringify())`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-api-route-json-helper",
    description: "Use `json()` from `@tanstack/react-start`, not \
                  `new Response(JSON.stringify(...))`.",
    remediation: "Replace `new Response(JSON.stringify(data), { headers: ... })` \
                  with `json(data)` — it sets the correct Content-Type and is \
                  safer to type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
