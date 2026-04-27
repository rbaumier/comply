//! nestjs-no-circular-injection — `forwardRef()` indicates circular DI.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-no-circular-injection",
    description: "`forwardRef()` reveals a circular dependency between providers.",
    remediation: "Refactor to break the cycle — extract shared logic into a third service.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
