//! consistent-date-clone

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "consistent-date-clone",
    description: "Prefer `new Date(date)` over `new Date(date.getTime())` for cloning.",
    remediation: "Remove the unnecessary `.getTime()` / `.valueOf()` call — `new Date(date)` already clones correctly.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
