//! Prefer `toReversed()` over `[...arr].reverse()` (ES2023).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-to-reversed",
    description: "Prefer `arr.toReversed()` over `[...arr].reverse()`.",
    remediation: "Replace `[...arr].reverse()` or `arr.slice().reverse()` with `arr.toReversed()` (ES2023).",
    severity: Severity::Warning,
    doc_url: Some(
        "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/toReversed",
    ),
    categories: &["e18e", "modernization"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
