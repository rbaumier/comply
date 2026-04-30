//! ts-no-unnecessary-parameter-property-assignment — disallow redundant
//! `this.x = x` when `x` is already a parameter property.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unnecessary-parameter-property-assignment",
    description: "Assigning `this.x = x` in a constructor is redundant when `x` is already a parameter property.",
    remediation: "Remove the redundant assignment — the parameter property already handles it.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://typescript-eslint.io/rules/no-unnecessary-parameter-property-assignment/",
    ),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
