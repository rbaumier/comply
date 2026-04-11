//! no-array-constructor

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-array-constructor",
    description: "`new Array()` is ambiguous — single numeric arg creates sparse array.",
    remediation: "Use array literals `[]` or `Array.from()` instead of `new Array(...)`. `new Array(3)` creates a sparse array of length 3, not `[3]`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
