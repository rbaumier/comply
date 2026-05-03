//! elysia-hooks-before-routes

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-hooks-before-routes",
    description: "Lifecycle hook chained after route definitions — Elysia hooks only apply to routes registered after them.",
    remediation: "Chain `.onBeforeHandle(...)`, `.onError(...)`, etc. before `.get(...)`/`.post(...)` so they apply to subsequent routes.",
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
