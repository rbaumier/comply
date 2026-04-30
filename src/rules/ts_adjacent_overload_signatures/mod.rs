//! ts-adjacent-overload-signatures — require that function overload signatures be consecutive.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-adjacent-overload-signatures",
    description: "Function overload signatures must be consecutive for readability.",
    remediation: "Move all overload signatures for the same function name next to each other.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/adjacent-overload-signatures/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
