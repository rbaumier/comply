//! prefer-called-with — prefer `toHaveBeenCalledWith(...)` over bare `toHaveBeenCalled()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-called-with",
    description: "Prefer `toHaveBeenCalledWith(...)` over bare `toHaveBeenCalled()` to assert specific arguments.",
    remediation: "Use toHaveBeenCalledWith() to assert specific arguments",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/jest-community/eslint-plugin-jest/blob/main/docs/rules/prefer-called-with.md"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
