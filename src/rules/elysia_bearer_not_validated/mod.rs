//! elysia-bearer-not-validated

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-bearer-not-validated",
    description: "Bearer token is destructured but never validated — handler accepts any token.",
    remediation: "Verify the bearer token (e.g. `await jwt.verify(bearer)`) and reject when invalid.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
