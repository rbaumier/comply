//! prefer-date-now — prefer `Date.now()` over `new Date().getTime()` and similar patterns.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-date-now",
    description: "Prefer `Date.now()` over `new Date().getTime()`, `+new Date()`, or `Number(new Date())`.",
    remediation: "Replace with `Date.now()`. It is clearer, avoids allocating a throwaway `Date` object, and is faster.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
