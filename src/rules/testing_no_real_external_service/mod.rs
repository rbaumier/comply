//! testing-no-real-external-service — flag `fetch`/`axios` calls to real external URLs in tests.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-real-external-service",
    description: "Test makes a real network call to an external service — intercept it with MSW instead.",
    remediation: "Mock the external service with MSW (or equivalent) — never hit the real endpoint from tests.",
    severity: Severity::Error,
    doc_url: Some("https://mswjs.io/"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
