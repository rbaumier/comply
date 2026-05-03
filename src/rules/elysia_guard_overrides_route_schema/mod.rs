//! elysia-guard-overrides-route-schema

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-guard-overrides-route-schema",
    description: "Routes nested in a `.guard({ body: ... })` block redeclare `body:` — the inner schema overrides the guard.",
    remediation: "Drop the `body:` from the inner route or remove it from the guard so a single source of truth validates the request body.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
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
