//! Prefer `Object.hasOwn()` over `hasOwnProperty` (ES2022).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-object-has-own",
    description: "Prefer `Object.hasOwn(obj, key)` over `obj.hasOwnProperty(key)`.",
    remediation: "Replace with `Object.hasOwn(obj, key)` (ES2022).",
    severity: Severity::Warning,
    doc_url: Some(
        "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Object/hasOwn",
    ),
    categories: &["e18e", "modernization"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
