//! import-no-amd

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "import-no-amd",
    description: "AMD `require` and `define` calls are forbidden.",
    remediation: "Use ES module `import` instead of AMD `require([...], fn)` or `define([...], fn)`.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-amd.md"),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
