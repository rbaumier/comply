//! regex-no-standalone-backslash

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-standalone-backslash",
    description: "Backslash followed by a non-special character in regex is an identity escape — likely a mistake.",
    remediation: "Remove the unnecessary backslash or use the correct escape sequence.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
