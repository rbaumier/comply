//! ts-explicit-module-boundary-types — require explicit return and argument types on exported functions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-module-boundary-types",
    description: "Exported functions and public class methods should have explicit return and parameter types.",
    remediation: "Add explicit return and parameter type annotations to exported functions.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-module-boundary-types/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
