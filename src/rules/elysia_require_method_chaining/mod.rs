//! elysia-require-method-chaining

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-require-method-chaining",
    description: "Elysia methods called on a stored variable instead of being chained — type inference is lost.",
    remediation: "Chain Elysia methods: `new Elysia().state(...).get(...)`. Each method returns a new type; breaking the chain loses type inference.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["type-safety", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
