//! ts-parameter-properties — require or disallow parameter properties
//! in class constructors (default: prefer class properties).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-parameter-properties",
    description: "Parameter properties mix declaration and assignment — prefer explicit class properties.",
    remediation: "Declare the property as a class field and assign it in the constructor body.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/parameter-properties/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
