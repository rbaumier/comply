//! prefer-query-selector

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-query-selector",
    description: "Prefer `.querySelector()` / `.querySelectorAll()` over legacy DOM query methods.",
    remediation: "Replace `.getElementById('x')` with `.querySelector('#x')`, and `.getElementsByClassName('x')` / `.getElementsByTagName('x')` / `.getElementsByName('x')` with `.querySelectorAll('.x')`. The `querySelector` API is more flexible and consistent.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
