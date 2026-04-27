//! elysia-resolve-outside-guard

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-resolve-outside-guard",
    description: "`.resolve()` is used at the Elysia chain top level instead of inside `.guard()`.",
    remediation: "Wrap the `.resolve(...)` in a `.guard({ ... }, app => app.resolve(...))` so the derived value is only added to scoped routes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
