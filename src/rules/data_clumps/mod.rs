//! data-clumps

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "data-clumps",
    description: "Same 3+ parameter names appear together in multiple function signatures.",
    remediation: "Extract the repeated parameter group into a value object / options type. Data clumps indicate a missing abstraction — e.g. `(host, port, protocol)` should be a `ConnectionConfig`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
