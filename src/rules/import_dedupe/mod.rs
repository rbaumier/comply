//! import-dedupe

mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "import-dedupe",
    description: "Duplicate named specifiers inside a single import statement.",
    remediation: "Remove the duplicate identifiers from the import specifier list.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/antfu/eslint-plugin-antfu/blob/main/src/rules/import-dedupe.md",
    ),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
