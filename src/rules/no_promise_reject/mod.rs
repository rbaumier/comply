//! no-promise-reject

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-promise-reject",
    description: "`Promise.reject()` makes error handling harder — prefer returning error values or throwing typed errors.",
    remediation: "Return a Result type, throw a typed error, or use `Promise.resolve()` with an error discriminant.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
