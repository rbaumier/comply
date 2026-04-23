//! ts-no-non-null-assertion — disallow non-null assertions (`value!`).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-non-null-assertion",
    description: "Non-null assertions (`value!`) suppress compiler checks and can hide real nullability bugs.",
    remediation: "Narrow the type with a check (`if (value)`), use optional chaining (`value?.x`), or rework the types so the value is known to be non-null.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-non-null-assertion/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
