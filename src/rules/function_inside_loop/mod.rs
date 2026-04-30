//! Detects function declarations inside loops — creates new function object each iteration.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "function-inside-loop",
    description: "Function declared inside loop creates new function object each iteration.",
    remediation: "Move the function outside the loop, or use a method reference.",
    severity: Severity::Warning,
    doc_url: Some("https://rules.sonarsource.com/javascript/RSPEC-1515"),
    categories: &["sonarjs", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
