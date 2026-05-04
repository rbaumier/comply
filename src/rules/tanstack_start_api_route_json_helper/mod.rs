//! tanstack-start-api-route-json-helper — API route handlers must use
//! `json()` from `@tanstack/react-start`, not `new Response(JSON.stringify())`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
