mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-prefer-map-single",
    description: "Forbid Result.gen wrapping a single transformation — use .map()/.andThen() instead.",
    remediation: "Replace Result.gen with a direct .map() or .andThen() when there is only one yield*.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
