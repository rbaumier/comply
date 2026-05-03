//! elysia-require-method-chaining

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-require-method-chaining",
    description: "Elysia methods called on a stored variable instead of being chained — type inference is lost.",
    remediation: "Chain Elysia methods: `new Elysia().state(...).get(...)`. Each method returns a new type; breaking the chain loses type inference.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["type-safety", "elysia"],
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
