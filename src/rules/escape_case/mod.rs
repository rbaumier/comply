//! escape-case — require uppercase hex digits in escape sequences.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "escape-case",
    description: "Use uppercase characters for the value of escape sequences.",
    remediation: "Replace lowercase hex digits in escape sequences with uppercase: \
                  `\\xff` -> `\\xFF`, `\\u00ff` -> `\\u00FF`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
