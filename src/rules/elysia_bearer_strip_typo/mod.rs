//! elysia-bearer-strip-typo

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-bearer-strip-typo",
    description: "`replace('Bearer', '')` leaves a leading space — should be `'Bearer '` (with trailing space).",
    remediation: "Use `.replace('Bearer ', '')` (note the trailing space) so the token is not whitespace-prefixed.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
