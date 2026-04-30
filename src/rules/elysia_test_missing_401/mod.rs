//! elysia-test-missing-401

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-test-missing-401",
    description: "Test file exercises an authenticated route but never asserts a 401 / Unauthorized response.",
    remediation: "Add a test case that sends the request without credentials and asserts the route returns 401.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
