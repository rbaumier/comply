//! react-no-dedup-filter-indexof — `arr.filter((v, i, a) => a.indexOf(v) === i)`.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-dedup-filter-indexof",
    description: "Deduping via `filter((v, i, a) => a.indexOf(v) === i)` is O(n²).",
    remediation: "Use `[...new Set(arr)]` — O(n).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
