//! no-alias-methods

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-alias-methods",
    description: "Jest/Vitest alias matchers should be replaced by their canonical form.",
    remediation: "Use canonical matcher name",
    severity: Severity::Warning,
    doc_url: Some("https://jestjs.io/docs/expect"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
