//! ts-no-invalid-void-type — disallow `void` outside of return types and
//! generic type arguments.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-invalid-void-type",
    description: "`void` is only valid as a return type or generic type argument.",
    remediation: "Use `undefined` instead of `void` outside of return types.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-invalid-void-type/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
