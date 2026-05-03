//! elysia-service-return-not-throw

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-service-return-not-throw",
    description: "`throw` in an Elysia service breaks typed error propagation — return `status(...)` instead.",
    remediation: "Return `status(code, message)` instead of throwing — Elysia convention for typed error propagation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "elysia"],
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
