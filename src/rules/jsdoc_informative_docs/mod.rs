//! jsdoc-informative-docs

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-informative-docs",
    description: "JSDoc description merely repeats the name of the symbol without adding useful information.",
    remediation: "Rewrite the JSDoc to explain *why* or *how* the symbol works, not just restate its name.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/gajus/eslint-plugin-jsdoc/blob/main/docs/rules/informative-docs.md",
    ),
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
