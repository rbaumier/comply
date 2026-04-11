//! prefer-immediate-return

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-immediate-return",
    description: "Variable is assigned and immediately returned.",
    remediation: "Return the expression directly: `return computeValue()` instead of `const result = computeValue(); return result;`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
