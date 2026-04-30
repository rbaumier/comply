//! inverted-assertion-arguments

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "inverted-assertion-arguments",
    description: "Expected and actual arguments in assertion are inverted.",
    remediation: "Use `expect(variable).toBe(literal)` — the expected value goes in the matcher, not in `expect()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
