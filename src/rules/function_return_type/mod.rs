//! function-return-type

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "function-return-type",
    description: "Functions should not return literals of different types.",
    remediation: "Ensure all return statements in a function return the same type of literal, or use a discriminated union type.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
