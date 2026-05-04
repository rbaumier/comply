//! ts-class-literal-property-style — enforce consistent literal property style on classes.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-class-literal-property-style",
    description: "Enforce that literals on classes are exposed in a consistent style (fields vs getters).",
    remediation: "Use `readonly` fields for literals instead of trivial getter methods (default), or vice versa.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/class-literal-property-style/"),
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
