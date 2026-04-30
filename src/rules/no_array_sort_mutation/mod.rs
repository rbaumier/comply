//! no-array-sort-mutation

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-array-sort-mutation",
    description: "Prefer `Array#toSorted()` over `Array#sort()` (mutates in place).",
    remediation: "Replace `.sort()` with `.toSorted()`. `Array#sort()` mutates the \
                  array in place which can cause subtle bugs. `Array#toSorted()` \
                  returns a new sorted array, leaving the original unchanged.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
