//! no-inconsistent-returns

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-inconsistent-returns",
    description: "Function has inconsistent returns — some paths return a value, others return nothing.",
    remediation: "Ensure every return path either returns a value or returns nothing. Mixing `return expr;` with bare `return;` or implicit returns is confusing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
