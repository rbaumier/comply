//! no-assign-mutated-array

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-assign-mutated-array",
    description: "Do not assign the result of a mutating array method (`sort`, `reverse`, `fill`).",
    remediation: "Use `toSorted()`, `toReversed()`, or spread before mutating: `[...arr].sort()`",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
