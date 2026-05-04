//! ts-class-methods-use-this — enforce that class methods utilize `this`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-class-methods-use-this",
    description: "Class methods that don't use `this` should be static or extracted to a standalone function.",
    remediation: "Add `static` to the method, move it to a standalone function, or use `this` in the body.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/class-methods-use-this"),
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
