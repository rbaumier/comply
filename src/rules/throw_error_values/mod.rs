//! throw-error-values — flag `throw 'string'` / `throw {}` / `throw 42`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "throw-error-values",
    description: "Only throw `Error` instances, not primitives or plain objects.",
    remediation: "Replace `throw 'msg'` or `throw { code: ... }` with \
                  `throw new Error('msg')`. Thrown non-Error values lose stack \
                  traces and break `instanceof Error` checks in catch handlers.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["error-handling"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
