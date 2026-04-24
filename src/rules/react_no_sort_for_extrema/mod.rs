//! react-no-sort-for-extrema — `.sort(...)[0]` / `sorted[length - 1]` for min/max.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-sort-for-extrema",
    description: "Sorting an array to pick only its first or last element is O(n log n) \
                  for work that can be done in O(n).",
    remediation: "Use a single-pass `Math.min` / `Math.max` or a manual fold.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
