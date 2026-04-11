//! group-exports

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "group-exports",
    description: "Multiple named export declarations — consolidate into a single export block.",
    remediation: "Gather all named exports into a single `export { … }` declaration at the bottom of the file instead of scattering `export` across multiple declarations.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
