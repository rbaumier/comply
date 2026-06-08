//! react-hook-form-use-no-memo — `useForm` files need a `"use no memo"` directive.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-hook-form-use-no-memo",
    description: "React Hook Form's `useForm` returns a proxy whose getters the React Compiler \
                  memoizes incorrectly, so a file calling `useForm` under the compiler needs a \
                  `\"use no memo\"` directive to opt that file out of memoization.",
    remediation: "Add a `\"use no memo\"` directive at the top of the file (or the component \
                  body) that calls `useForm`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

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
