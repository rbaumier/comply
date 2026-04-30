//! valid-describe-callback — require describe callbacks to be sync, parameter-less, and not return a value.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "valid-describe-callback",
    description: "`describe` callback must be a synchronous function with no parameters and no return value.",
    remediation: "describe callback must be sync function with no parameters and no return",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jest-community/eslint-plugin-jest/blob/main/docs/rules/valid-describe-callback.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
