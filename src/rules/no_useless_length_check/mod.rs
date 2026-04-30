//! no-useless-length-check — flag redundant `.length` checks before `.some()`/`.every()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-length-check",
    description: "Disallow useless array length check.",
    remediation: "Remove the redundant `.length` guard. `Array#some()` \
                  already returns `false` for an empty array, and \
                  `Array#every()` already returns `true` for an empty array. \
                  The length check adds no value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
