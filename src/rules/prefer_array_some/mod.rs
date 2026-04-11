//! prefer-array-some

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-some",
    description: "Prefer `.some(…)` over `.filter(…).length` checks.",
    remediation: "Replace `.filter(…).length > 0` with `.some(…)` — it short-circuits.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
