//! regex-optimal-lookaround-quantifier

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-optimal-lookaround-quantifier",
    description: "Quantified expression at the edge of a lookaround should only match a constant number of times.",
    remediation: "Remove or simplify the quantifier at the start/end of the lookaround expression.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/optimal-lookaround-quantifier.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
