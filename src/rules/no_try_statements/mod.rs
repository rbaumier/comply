//! no-try-statements

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-try-statements",
    description: "`try` blocks obscure error flow — prefer Result types or explicit error handling.",
    remediation: "Use a Result/Either type, or a wrapper function that returns `{ data, error }` tuples instead of try/catch.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
