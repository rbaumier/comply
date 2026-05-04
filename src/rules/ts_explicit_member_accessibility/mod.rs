//! ts-explicit-member-accessibility — require explicit accessibility modifiers
//! (`public`/`private`/`protected`) on class members.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-member-accessibility",
    description: "Require explicit accessibility modifiers on class properties and methods.",
    remediation: "Prefix every class property and method with `public`, `private`, \
                  or `protected`. Explicit accessibility makes the intended API \
                  surface obvious without requiring readers to memorize defaults.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-member-accessibility/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
