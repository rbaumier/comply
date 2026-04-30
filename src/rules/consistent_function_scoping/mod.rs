//! consistent-function-scoping

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "consistent-function-scoping",
    description: "Nested function does not capture any variable from its parent scope and could be hoisted.",
    remediation: "Move the inner function to the outer scope or module level. Functions that don't close over parent state belong at the top level where they're easier to test and reuse.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/sindresorhus/eslint-plugin-unicorn/blob/main/docs/rules/consistent-function-scoping.md",
    ),
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
