//! Detects functions that return inconsistent types.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "function-return-type",
    description: "Detects functions returning inconsistent types across branches.",
    remediation: "Ensure all return paths return the same type, or use a discriminated union.",
    severity: Severity::Warning,
    doc_url: Some("https://rules.sonarsource.com/javascript/RSPEC-3800"),
    categories: &["sonarjs", "code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
