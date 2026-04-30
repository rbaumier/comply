//! no-array-reverse

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-array-reverse",
    description: "`Array#reverse()` mutates the array in place.",
    remediation: "Use `.toReversed()` instead — it returns a new array without mutating the original.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
