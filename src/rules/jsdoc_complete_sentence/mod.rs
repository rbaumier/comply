//! jsdoc-complete-sentence

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-complete-sentence",
    description: "JSDoc descriptions must start with a capital letter and end with punctuation.",
    remediation: "Capitalize the first letter and end the description with `.`, `!`, or `?`. Complete sentences read better in generated docs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
