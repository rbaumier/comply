mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "valid-expect-in-promise",
    description: "Assertions in Promise `.then()`/`.catch()` must be returned or awaited.",
    remediation: "Return or await the Promise chain containing the `expect()` call.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jest-community/eslint-plugin-jest/blob/main/docs/rules/valid-expect-in-promise.md",
    ),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
