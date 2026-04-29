//! no-magic-numbers — disallow magic numbers.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-magic-numbers",
    description: "Magic numbers make code harder to understand — use named constants instead.",
    remediation: "Extract the number into a named `const`. TS enums, numeric literal types, `readonly` properties, and common values (0, 1, -1) are allowed.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-magic-numbers"),
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
