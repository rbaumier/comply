//! no-for-loop — prefer `for-of` over classic indexed `for` loops.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-for-loop",
    description: "Use a `for-of` loop instead of this `for` loop.",
    remediation: "Replace `for (let i = 0; i < arr.length; i++)` with \
                  `for (const item of arr)`. If the index is needed, use \
                  `for (const [i, item] of arr.entries())`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
