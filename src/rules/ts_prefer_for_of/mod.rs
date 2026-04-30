//! ts-prefer-for-of — prefer `for-of` over index-only `for` loops.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-for-of",
    description: "A `for` loop whose index is only used for array access can be a simpler `for-of`.",
    remediation: "Replace the `for` loop with `for (const item of array)`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-for-of/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
