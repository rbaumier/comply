//! regex-no-empty-lookaround

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-lookaround",
    description: "Empty lookaround (`(?=)`, `(?!)`, `(?<=)`, `(?<!)`) always matches or always fails — likely a mistake.",
    remediation: "Add a pattern inside the lookaround or remove it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
