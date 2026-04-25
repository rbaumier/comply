//! api-list-requires-pagination — GET list handlers must expose a
//! pagination parameter.
//!
//! Unbounded list endpoints are a latent DoS: a single call can fetch
//! the full table and pin memory / DB resources. Requiring a pagination
//! primitive (`limit`, `cursor`, `page`, `pageSize`, `offset`,
//! `per_page`) forces the author to think about result-set size at the
//! API boundary.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "api-list-requires-pagination",
    description: "List endpoints must support pagination to prevent unbounded result sets.",
    remediation: "Add `limit`/`cursor` or `page`/`pageSize` parameters to the handler.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
