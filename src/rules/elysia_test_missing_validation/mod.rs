//! elysia-test-missing-validation

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-test-missing-validation",
    description: "Test file declares a body schema but never asserts a 400/422 validation error.",
    remediation: "Add a test case that sends an invalid payload and asserts the route returns 400 (or 422 in `aot:false` mode).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
