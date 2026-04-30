//! ts-no-unused-expressions — flag expression statements that do nothing.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unused-expressions",
    description: "Expression statements that produce a value but discard it are likely mistakes.",
    remediation: "Assign the result to a variable, use it as a condition, or remove the statement.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-unused-expressions"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
