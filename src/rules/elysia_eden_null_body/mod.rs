//! elysia-eden-null-body

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-eden-null-body",
    description: "Eden Treaty calls pass `undefined` as the body argument; should be `null`.",
    remediation: "Use `null` instead of `undefined` for an empty body in Eden mutations: `treaty.path.post(null, options)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
