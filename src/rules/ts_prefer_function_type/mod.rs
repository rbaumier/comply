//! ts-prefer-function-type — prefer function types over interfaces with
//! only a call signature.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-function-type",
    description: "An interface with only a call signature should be a function type.",
    remediation: "Replace `interface Fn { (): T }` with `type Fn = () => T`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-function-type/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
