//! ts-no-magic-numbers — disallow magic numbers in TypeScript code.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-magic-numbers",
    description: "Magic numbers make code harder to understand — use named constants instead.",
    remediation: "Extract the number into a named `const`. TS enums, numeric literal types, `readonly` properties, and common values (0, 1, -1) are allowed.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-magic-numbers"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
