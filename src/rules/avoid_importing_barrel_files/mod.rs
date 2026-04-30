//! avoid-importing-barrel-files

mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "avoid-importing-barrel-files",
    description: "Importing from a barrel (`index`) file in the same project hurts tree-shaking and inflates startup cost.",
    remediation: "Import directly from the module that defines the symbol instead of going through the barrel.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/thepassle/eslint-plugin-barrel-files"),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
