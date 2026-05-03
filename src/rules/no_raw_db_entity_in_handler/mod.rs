//! no-raw-db-entity-in-handler

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-raw-db-entity-in-handler",
    description: "Route handlers should not return raw DB queries directly.",
    remediation: "Map the DB entity to a DTO before returning from the route handler. Returning raw DB entities leaks internal schema details, couples the API shape to the database, and makes it easy to accidentally expose sensitive columns.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
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
