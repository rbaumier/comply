//! no-mutating-assign

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-mutating-assign",
    description: "Disallow `Object.assign(target, ...)` when `target` is not an empty object literal — it mutates the target in place.",
    remediation: "Use spread syntax `{...target, ...source}` or `Object.assign({}, target, source)` to produce a new object instead of mutating.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
