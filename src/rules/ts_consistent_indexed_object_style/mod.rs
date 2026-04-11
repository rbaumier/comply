//! ts-consistent-indexed-object-style — prefer `Record<K, V>` over index signatures.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-indexed-object-style",
    description: "Prefer `Record<K, V>` over manual index signature `{ [key: K]: V }` for consistency.",
    remediation: "Replace the index signature with `Record<K, V>`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-indexed-object-style/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
