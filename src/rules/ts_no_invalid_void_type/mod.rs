//! ts-no-invalid-void-type — disallow `void` outside of return types and
//! generic type arguments.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-invalid-void-type",
    description: "`void` is only valid as a return type or generic type argument.",
    remediation: "Use `undefined` instead of `void` outside of return types.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-invalid-void-type/"),
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
