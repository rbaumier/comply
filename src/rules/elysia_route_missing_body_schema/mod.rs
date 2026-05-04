//! elysia-route-missing-body-schema

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-route-missing-body-schema",
    description: "Elysia route handler reads `body` but the route has no `body:` schema.",
    remediation: "Declare `body: t.Object({...})` (or a registered model name) in the route options so Elysia validates and types the request body.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["validation", "elysia"],
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
