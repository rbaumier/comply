//! ts-consistent-type-assertions — enforce consistent usage of type assertions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-type-assertions",
    description: "Enforce consistent type assertion style (`as T` vs `<T>`).",
    remediation: "Use `as T` syntax instead of angle-bracket `<T>` assertions for consistency.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-type-assertions/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
