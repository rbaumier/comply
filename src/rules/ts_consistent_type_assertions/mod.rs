//! ts-consistent-type-assertions — enforce consistent usage of type assertions.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-consistent-type-assertions",
    description: "Enforce consistent type assertion style (`as T` vs `<T>`).",
    remediation: "Use `as T` syntax instead of angle-bracket `<T>` assertions for consistency.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/consistent-type-assertions/"),
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
