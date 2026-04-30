//! prefer-structured-clone — prefer `structuredClone()` over `JSON.parse(JSON.stringify())`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-structured-clone",
    description: "Prefer `structuredClone(…)` over `JSON.parse(JSON.stringify(…))` for deep cloning.",
    remediation: "Replace `JSON.parse(JSON.stringify(x))` with `structuredClone(x)`. \
                  `structuredClone` handles circular references, typed arrays, and \
                  other values that JSON serialization silently drops or corrupts.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
