//! Prefer `Array.from(iter, fn)` over `[...iter].map(fn)`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-from-map",
    description: "Prefer `Array.from(iter, mapFn)` over `[...iter].map(mapFn)`.",
    remediation: "Use `Array.from(iterable, mapFn)` to avoid intermediate array allocation.",
    severity: Severity::Warning,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/from"),
    categories: &["unicorn", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
