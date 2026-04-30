//! ts-consistent-generic-constructors — enforce generic type arguments on constructor call site.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-generic-constructors",
    description: "Generic type arguments should be on the constructor, not the variable annotation.",
    remediation: "Move the type argument from the type annotation to the constructor: `new Map<K, V>()` instead of `const m: Map<K, V> = new Map()`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-generic-constructors/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
