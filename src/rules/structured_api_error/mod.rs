//! structured-api-error

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "structured-api-error",
    description: "Bare `new Error()` in route handlers — use structured errors.",
    remediation: "Replace `new Error(\"message\")` with a structured error containing `{ type, code, status, detail }`. Bare Error messages are not machine-parseable and lack HTTP status context.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
