//! elysia-response-status-mismatch

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-response-status-mismatch",
    description: "Handler returns a status code that is not declared in the route's `response:` schema.",
    remediation: "Add the status key (e.g. `404: t.Object({ message: t.String() })`) to the route's `response:` map.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
