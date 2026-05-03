//! ts-no-unused-expressions — flag expression statements that do nothing.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unused-expressions",
    description: "Expression statements that produce a value but discard it are likely mistakes.",
    remediation: "Assign the result to a variable, use it as a condition, or remove the statement.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-unused-expressions"),
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
