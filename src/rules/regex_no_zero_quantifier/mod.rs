//! regex-no-zero-quantifier

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-zero-quantifier",
    description: "Quantifier `{0}` or `{0,0}` matches nothing — the pattern is likely a mistake.",
    remediation: "Remove the quantified sub-expression or fix the quantifier.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
