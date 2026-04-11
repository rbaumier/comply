//! ts-no-restricted-imports — disallow specified module imports.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-restricted-imports",
    description: "Some modules should not be imported due to deprecation, side effects, or project conventions.",
    remediation: "Replace the restricted import with the recommended alternative.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-restricted-imports"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
