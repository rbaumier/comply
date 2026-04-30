//! no-uniq-key

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-uniq-key",
    description: "Non-unique key in JSX list — `Math.random()`, `Date.now()`, or `uuid()` create new keys every render.",
    remediation: "Use a stable, unique identifier from the data (e.g., `item.id`). Random keys destroy React's reconciliation and cause performance issues.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
