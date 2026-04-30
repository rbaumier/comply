//! prefer-early-return

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-early-return",
    description: "Function body is wrapped in a single `if` — invert it as a guard clause.",
    remediation: "Invert the condition and return early: `if (!cond) return; ...` — reduces nesting and clarifies the happy path.",
    severity: Severity::Warning,
    doc_url: Some("https://eslint.org/docs/latest/rules/no-else-return"),
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
