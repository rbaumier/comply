//! no-raw-db-entity-in-handler

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
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
    crate::register_ts_family!(META, typescript)
}
