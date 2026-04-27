//! elysia-macro-named-inference

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-macro-named-inference",
    description: "`.macro({ ... })` bulk form blocks cross-macro type inference.",
    remediation: "Use the named form `.macro('name', { ... })` so other macros can `resolve` against this macro's output type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
