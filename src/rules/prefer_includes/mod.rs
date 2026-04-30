//! prefer-includes

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-includes",
    description: "Prefer `.includes(x)` over `.indexOf(x) !== -1`.",
    remediation: "Replace `.indexOf(x) !== -1` or `.indexOf(x) >= 0` with `.includes(x)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
