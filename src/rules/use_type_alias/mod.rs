//! use-type-alias

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "use-type-alias",
    description: "Repeated complex inline type annotations should be extracted into a type alias.",
    remediation: "Create a `type` alias for the repeated annotation and use it in all positions.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
