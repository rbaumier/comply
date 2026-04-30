//! no-self-import

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-self-import",
    description: "Module imports itself.",
    remediation: "Remove the self-import. A module should never import from itself — it causes circular dependency issues.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
