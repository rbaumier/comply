//! react-no-chained-filter-map-reduce — 3+ chained `.filter/.map/.reduce` calls.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-chained-filter-map-reduce",
    description: "Three or more consecutive `.filter`/`.map`/`.reduce` calls walk the array \
                  multiple times and allocate intermediate arrays.",
    remediation: "Collapse the chain into a single `for`/`reduce` pass or use a lazy iterator.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
