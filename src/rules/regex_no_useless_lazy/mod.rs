//! regex-no-useless-lazy

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-lazy",
    description: "Lazy quantifier has no effect when the quantified token can only match a single length.",
    remediation: "Remove the `?` after the quantifier \u{2014} it has no effect here.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
