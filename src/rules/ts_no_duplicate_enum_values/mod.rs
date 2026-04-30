//! ts-no-duplicate-enum-values — flag duplicate values in enum declarations.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-duplicate-enum-values",
    description: "Duplicate enum member values cause silent shadowing at runtime.",
    remediation: "Assign unique values to each enum member.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
