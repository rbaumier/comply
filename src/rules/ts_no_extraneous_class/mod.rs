//! ts-no-extraneous-class — disallow classes used as namespaces.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-extraneous-class",
    description: "Classes with only static members or an empty body should be plain objects or modules.",
    remediation: "Use a module/namespace, plain object, or standalone functions instead.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-extraneous-class/"),
    categories: &["typescript"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
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
