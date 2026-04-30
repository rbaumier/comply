//! prefer-mock-promise-shorthand — flag `.mockImplementation(() => Promise.resolve/reject(x))`
//! and suggest the shorthand `.mockResolvedValue(x)` / `.mockRejectedValue(x)`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-mock-promise-shorthand",
    description: "Prefer `.mockResolvedValue(x)` / `.mockRejectedValue(x)` over `.mockImplementation(() => Promise.resolve/reject(x))`.",
    remediation: "Use mockResolvedValue/mockRejectedValue instead",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/sindresorhus/eslint-plugin-unicorn/blob/main/docs/rules/prefer-mock-promise-shorthand.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
