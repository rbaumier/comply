//! import-prefer-default-export

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "import-prefer-default-export",
    description: "Prefer a default export when a module has a single export.",
    remediation: "Use `export default` instead of a single named export.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/prefer-default-export.md"),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
