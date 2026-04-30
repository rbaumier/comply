//! no-test-imports-in-prod

mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-test-imports-in-prod",
    description: "Production sources must not import test or mock files.",
    remediation: "Move the shared logic out of the test file, or keep the import inside a test file only.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports", "testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
