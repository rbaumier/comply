//! Prefer `Array(n).fill(v)` over `Array.from({length: n}, () => v)`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-fill",
    description: "Prefer `Array(n).fill(v)` over `Array.from({length: n}, () => v)` for constant fills.",
    remediation: "Use `Array(n).fill(value)` for simpler constant array initialization.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/fill",
    ),
    categories: &["unicorn", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
