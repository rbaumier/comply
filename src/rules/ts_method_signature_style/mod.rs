//! ts-method-signature-style — enforce property signature for methods in interfaces.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-method-signature-style",
    description: "Shorthand method signatures in interfaces are less safe than property signatures — they allow unsafe variance.",
    remediation: "Use a property signature with a function type: `foo: (x: string) => void` instead of `foo(x: string): void`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/method-signature-style"),
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
