//! Prefer `**` operator over `Math.pow()` (ES2016).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-exponentiation-operator",
    description: "Prefer `x ** y` over `Math.pow(x, y)`.",
    remediation: "Replace `Math.pow(x, y)` with `x ** y` (ES2016).",
    severity: Severity::Warning,
    doc_url: Some("https://eslint.org/docs/latest/rules/prefer-exponentiation-operator"),
    categories: &["e18e", "modernization"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
