//! require-to-throw-message — require a message argument on `.toThrow()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "require-to-throw-message",
    description: "Require an expected error message argument on `.toThrow()` / `.toThrowError()`.",
    remediation: "Provide expected error message to toThrow()",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jest-community/eslint-plugin-jest/blob/main/docs/rules/require-to-throw-message.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
