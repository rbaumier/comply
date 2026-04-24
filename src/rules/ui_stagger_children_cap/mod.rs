//! ui-stagger-children-cap — `staggerChildren` values above 0.05s feel slow.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-stagger-children-cap",
    description: "`staggerChildren` should stay ≤ 0.05s (50ms); larger values make lists feel sluggish.",
    remediation: "Reduce `staggerChildren` to 0.05 or less, or drop it entirely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
