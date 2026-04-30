//! prefer-array-index-of

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-index-of",
    description: "Prefer `.indexOf(val)` over `.findIndex(x => x === val)`.",
    remediation: "Replace `.findIndex(x => x === val)` with `.indexOf(val)` for simple equality checks.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
