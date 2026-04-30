//! Prefer `toSpliced()` over `slice().splice()` patterns.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-to-spliced",
    description: "Prefer `toSpliced()` over `slice().splice()` for immutable splice.",
    remediation: "Use `arr.toSpliced(start, deleteCount, ...items)` instead.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/toSpliced",
    ),
    categories: &["unicorn", "es2023"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
