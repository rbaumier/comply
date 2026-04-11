//! no-unnecessary-array-splice-count

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-array-splice-count",
    description: "Disallow unnecessary `.length` or `Infinity` as the count argument of `Array#splice()` / `Array#toSpliced()`.",
    remediation: "Remove the second argument: `.splice(start)` deletes all elements from `start` to the end.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
