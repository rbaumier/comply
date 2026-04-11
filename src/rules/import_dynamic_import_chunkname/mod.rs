//! import-dynamic-import-chunkname

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "import-dynamic-import-chunkname",
    description: "Dynamic imports require a leading `webpackChunkName` comment.",
    remediation: "Add a `/* webpackChunkName: \"name\" */` comment before the import source.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/dynamic-import-chunkname.md"),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
