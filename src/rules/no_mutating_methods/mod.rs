//! no-mutating-methods

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-mutating-methods",
    description: "Disallow array mutating methods (push, pop, shift, unshift, splice, sort, reverse, fill, copyWithin).",
    remediation: "Use non-mutating alternatives: spread (`[...arr, x]`), `slice`, `toSorted`, `toReversed`, `toSpliced`, `filter`, `map`, or `concat`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
