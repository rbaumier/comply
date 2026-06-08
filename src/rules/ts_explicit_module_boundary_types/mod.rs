//! ts-explicit-module-boundary-types — require explicit types on the
//! arguments of exported functions.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-module-boundary-types",
    description: "Require explicit argument types on exported functions and class methods.",
    remediation: "Annotate every parameter of any exported function. Exported \
                  signatures are the module's public contract — inferred types drift \
                  silently as the implementation changes and surprise downstream \
                  consumers. Return types are handled by \
                  `ts-explicit-function-return-type`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-module-boundary-types/"),
    categories: &["typescript"],

    skip_in_test_dir: false,
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
