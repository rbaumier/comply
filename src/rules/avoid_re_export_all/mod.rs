mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "avoid-re-export-all",
    description: "`export * from '...'` re-exports hide the module's public surface and break tree-shaking.",
    remediation: "Use named exports instead of re-exporting all",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
