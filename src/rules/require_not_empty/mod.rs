//! require-not-empty

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "require-not-empty",
    description: "Module specifiers must not be empty strings.",
    remediation: "Provide a valid module path",
    severity: Severity::Error,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
