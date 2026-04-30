//! regex-no-slow-pattern

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-slow-pattern",
    description: "Regex has nested quantifiers that can cause catastrophic backtracking (ReDoS).",
    remediation: "Refactor to avoid nested quantifiers like `(a+)+`, `(a*)*`, `(a+)*`, `(.*)*`. Use atomic groups, possessive quantifiers, or restructure the pattern.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
