//! elysia-transform-no-schema

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-transform-no-schema",
    description: "`transform` mutates `body` without a declared `body:` schema — input is unchecked.",
    remediation: "Declare a `body:` schema for the route or scope before transforming the body, so Elysia validates the shape before mutation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
