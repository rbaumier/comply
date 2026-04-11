//! prefer-spread

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-spread",
    description: "Prefer the spread operator over `Array.from()`, `Array#concat()`, and `Array#slice()`.",
    remediation: "Use `[...x]` instead of `Array.from(x)`, `[...arr, ...other]` instead of `arr.concat(other)`, and `[...arr]` instead of `arr.slice()`. The spread syntax is more idiomatic.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
