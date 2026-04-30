//! test-check-exception

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "test-check-exception",
    description: "`.toThrow()` without specifying what to check.",
    remediation: "Specify the expected error: `.toThrow(TypeError)`, `.toThrow('message')`, or `.toThrow(/regex/)`. Bare `.toThrow()` passes for any error, hiding bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
