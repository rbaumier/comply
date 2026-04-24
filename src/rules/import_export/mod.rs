//! import-export rule — flag duplicate export names within a single module.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "import-export",
    description: "No duplicate export names within a module.",
    remediation: "Remove or rename the duplicate export.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/export.md",
    ),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
