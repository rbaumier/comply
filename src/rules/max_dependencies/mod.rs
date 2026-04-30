//! max-dependencies

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "max-dependencies",
    description: "File has too many import dependencies — consider splitting.",
    remediation: "Reduce the number of imports by extracting logic into sub-modules or by re-evaluating whether all dependencies are necessary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
