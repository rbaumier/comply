mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-no-rewrap-error",
    description: "Forbid `return Result.err(result.error)` when `return result` suffices.",
    remediation: "Return the existing Result directly instead of re-wrapping its error.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
