//! ts-prefer-enum-initializers — require each enum member to be explicitly
//! initialized.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-enum-initializers",
    description: "Enum members without explicit values are fragile — reordering changes their runtime value.",
    remediation: "Assign an explicit value to each enum member.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-enum-initializers/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
