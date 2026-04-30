//! redundant-type-aliases

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "redundant-type-aliases",
    description: "`type X = Y` where Y is a single type adds no structure — it's just renaming.",
    remediation: "Use the original type directly, or add structure (union, intersection, generics) to justify the alias.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
