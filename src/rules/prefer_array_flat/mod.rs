//! prefer-array-flat

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-flat",
    description: "Prefer `.flat()` over legacy array flattening techniques.",
    remediation: "Replace `[].concat(…arr)` or `.reduce((a,b) => a.concat(b), [])` with `.flat()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
