//! ts-parameter-properties — require or disallow parameter properties
//! in class constructors (default: prefer class properties).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-parameter-properties",
    description: "Parameter properties mix declaration and assignment — prefer explicit class properties.",
    remediation: "Declare the property as a class field and assign it in the constructor body.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/parameter-properties/"),
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
