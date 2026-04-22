//! no-invalid-this — ports typescript-eslint's
//! `@typescript-eslint/no-invalid-this`: flag `this` used outside of a
//! class body or a function that can legally bind `this`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-invalid-this",
    description: "`this` keyword used outside of a class method or `function` body.",
    remediation: "Move the logic into a class method, a `function` that is bound/called with \
                  the desired receiver, or capture the needed value in a closure variable.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-invalid-this"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
