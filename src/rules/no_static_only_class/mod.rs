//! no-static-only-class — flag classes that contain only static members.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-static-only-class",
    description: "Disallow classes that only have static members.",
    remediation: "Replace the class with plain exported functions or an object \
                  literal. Static-only classes add indirection without benefit \
                  — they cannot be instantiated meaningfully and prevent \
                  tree-shaking.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
