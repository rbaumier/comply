//! layer-import-boundary

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "layer-import-boundary",
    description: "Imports that cross hexagonal architecture layers break \
                  dependency inversion and make the domain untestable.",
    remediation: "Domain must not import from infrastructure or application. \
                  Application must not import from infrastructure. \
                  Use dependency injection or ports/adapters instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["architecture"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
