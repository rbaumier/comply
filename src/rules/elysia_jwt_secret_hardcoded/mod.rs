//! elysia-jwt-secret-hardcoded

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-jwt-secret-hardcoded",
    description: "JWT secret is a hardcoded string literal — leaks via source control.",
    remediation: "Read the secret from `process.env.JWT_SECRET` or a secret manager, never hardcode it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
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
