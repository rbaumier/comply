//! elysia-jwt-verify-unchecked

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-jwt-verify-unchecked",
    description: "`jwt.verify(...)` result is used without checking for failure (returns falsy when invalid).",
    remediation: "Check the result: `const payload = await jwt.verify(...); if (!payload) return status(401);`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
