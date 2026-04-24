mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-no-mixed-throw",
    description: "A function returning Result<...> must not contain throw (except inside Result.try).",
    remediation: "Return Result.err(...) instead of throwing inside Result-returning functions.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
