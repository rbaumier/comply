//! prefer-response-static-json — prefer `Response.json()` over `new Response(JSON.stringify())`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-response-static-json",
    description: "Prefer `Response.json()` over `new Response(JSON.stringify())`.",
    remediation: "Replace `new Response(JSON.stringify(data), ...)` with \
                  `Response.json(data, ...)`. The static method sets the \
                  `Content-Type` header automatically and is more readable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
