//! regex-no-useless-two-nums-quantifier

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-two-nums-quantifier",
    description: "Quantifier `{n,n}` is equivalent to `{n}` — the range is redundant.",
    remediation: "Simplify `{3,3}` to `{3}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
