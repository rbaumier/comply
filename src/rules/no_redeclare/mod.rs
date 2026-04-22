//! no-redeclare — ports typescript-eslint's `@typescript-eslint/no-redeclare`.
//! Flag `var` / `function` names declared more than once in the same scope.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-redeclare",
    description: "Variable or function is declared more than once in the same scope.",
    remediation: "Rename the duplicate or remove it. Use `let`/`const` at block scope so the \
                  engine prevents accidental redeclaration.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-redeclare"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
