//! ts-no-invalid-this — disallow `this` outside classes or class-like objects.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-invalid-this",
    description: "`this` used outside a class or class-like object is likely a bug.",
    remediation: "Move the code into a class method, or use an explicit parameter instead of `this`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-invalid-this"),
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
