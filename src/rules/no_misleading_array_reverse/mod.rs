//! no-misleading-array-reverse

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-misleading-array-reverse",
    description: "`.reverse()`, `.sort()`, `.fill()`, `.splice()` mutate in place — assigning or returning the result is misleading.",
    remediation: "These methods mutate the original array and return the same reference. Use `[...arr].reverse()` or `arr.toReversed()` to avoid mutating the original.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
