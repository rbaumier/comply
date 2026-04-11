//! no-raw-db-entity-in-handler

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
