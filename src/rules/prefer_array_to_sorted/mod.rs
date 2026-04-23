//! Prefer `toSorted()` over `[...arr].sort()` (ES2023).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-to-sorted",
    description: "Prefer `arr.toSorted()` over `[...arr].sort()`.",
    remediation: "Replace `[...arr].sort()` or `arr.slice().sort()` with `arr.toSorted()` (ES2023).",
    severity: Severity::Warning,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/toSorted"),
    categories: &["e18e", "modernization"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
