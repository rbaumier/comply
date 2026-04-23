//! Requires explicit type arguments on known generic APIs.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "require-type-arguments",
    description: "Requires explicit type arguments on known generic APIs.",
    remediation: "Add explicit type parameters: `useState<string>()`, `new Map<K, V>()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
