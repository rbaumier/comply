//! jsdoc-needs-description

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-needs-description",
    description: "JSDoc block has tags but no description.",
    remediation: "Add a prose description to the JSDoc block. Tags alone don't explain what the function does or why.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "jsdoc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
