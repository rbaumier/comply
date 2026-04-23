//! no-identical-title — flag duplicate `describe`/`test`/`it` titles within the same scope.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-identical-title",
    description: "Duplicate test or describe titles within the same scope hide which assertion actually failed.",
    remediation: "Use unique test titles.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jest-community/eslint-plugin-jest/blob/main/docs/rules/no-identical-title.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
