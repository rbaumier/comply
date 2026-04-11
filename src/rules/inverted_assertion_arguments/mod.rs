//! inverted-assertion-arguments

mod typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "inverted-assertion-arguments",
    description: "Expected and actual arguments in assertion are inverted.",
    remediation: "Use `expect(variable).toBe(literal)` — the expected value goes in the matcher, not in `expect()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
