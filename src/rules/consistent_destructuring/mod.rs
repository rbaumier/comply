//! consistent-destructuring

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "consistent-destructuring",
    description: "Use destructured variables over properties.",
    remediation: "A property was already destructured from this object — destructure \
                  this property too instead of accessing it via dot notation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
