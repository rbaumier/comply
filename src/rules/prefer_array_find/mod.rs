//! prefer-array-find

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-find",
    description: "Prefer `.find(…)` over `.filter(…)[0]` or `.filter(…).at(0)`.",
    remediation: "Replace `.filter(…)[0]` with `.find(…)` to short-circuit on the first match.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
