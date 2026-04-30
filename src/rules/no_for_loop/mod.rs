//! no-for-loop — prefer `for-of` over classic indexed `for` loops.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

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
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
