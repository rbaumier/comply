//! ts-no-empty-function — disallow empty functions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-empty-function",
    description: "Empty functions are often a sign of incomplete refactoring.",
    remediation: "Add a comment explaining why the function is intentionally empty, or remove it.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-empty-function/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
