//! ts-no-this-alias — disallow aliasing `this`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-this-alias",
    description: "Assigning `this` to a variable is a legacy pattern — use arrow functions instead.",
    remediation: "Use an arrow function to capture `this` lexically.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-this-alias/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
