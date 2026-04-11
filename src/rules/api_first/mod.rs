//! api-first

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "api-first",
    description: "Route handler files should define an API schema.",
    remediation: "Define the API schema before the handler using `z.object`, `createRoute`, or `zodValidator`. API-first design ensures the contract is documented and validated before implementation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
