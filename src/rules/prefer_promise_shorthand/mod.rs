//! prefer-promise-shorthand

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-promise-shorthand",
    description: "`new Promise` wrapping a single `resolve`/`reject` call — use `Promise.resolve`/`Promise.reject` instead.",
    remediation: "Replace `new Promise((resolve) => resolve(x))` with `Promise.resolve(x)` and `new Promise((_, reject) => reject(x))` with `Promise.reject(x)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
