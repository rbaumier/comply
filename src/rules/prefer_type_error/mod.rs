//! prefer-type-error — enforce throwing TypeError in type-checking conditions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-type-error",
    description: "Use `TypeError` instead of `Error` in type-checking conditions.",
    remediation: "When throwing inside an `if` that performs a type check \
                  (typeof, instanceof, Array.isArray, etc.), use `new TypeError()` \
                  instead of `new Error()` to signal the caller passed a wrong type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
