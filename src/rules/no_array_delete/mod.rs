//! no-array-delete

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-array-delete",
    description: "`delete` on an array element creates a sparse hole instead of removing.",
    remediation: "Use `Array.prototype.splice()` to remove elements: `arr.splice(index, 1)` instead of `delete arr[index]`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
