//! import-no-webpack-loader-syntax

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "import-no-webpack-loader-syntax",
    description: "Webpack loader syntax in imports is forbidden.",
    remediation: "Do not use `!` import syntax to configure webpack loaders. Use webpack config instead.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-webpack-loader-syntax.md"),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
