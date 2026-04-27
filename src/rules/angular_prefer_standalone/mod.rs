//! angular-prefer-standalone — prefer standalone components over NgModule.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-prefer-standalone",
    description: "Standalone components don't need NgModule declarations (Angular 15+).",
    remediation: "Add `standalone: true` and import dependencies via `imports: [...]`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
