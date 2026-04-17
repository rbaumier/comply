//! regex-no-octal

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-octal",
    description: "Octal escapes in regex (`\\1`, `\\12`) are ambiguous — they could be backreferences or octal character codes.",
    remediation: "Use named backreferences (`\\k<name>`) or explicit Unicode escapes (`\\u{...}`) instead of bare octal sequences.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
