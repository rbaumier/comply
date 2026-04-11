//! no-array-method-this-argument

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-array-method-this-argument",
    description: "Do not use the `thisArg` parameter in array methods.",
    remediation: "Remove the second argument from the array method call. Use `.bind()` or an arrow function to bind context instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
