//! explicit-length-check

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "explicit-length-check",
    description: "Enforce explicitly comparing the `length` or `size` property of a value.",
    remediation: "Use `arr.length > 0` instead of `arr.length` and `arr.length === 0` instead of `!arr.length`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
