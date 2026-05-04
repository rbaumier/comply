//! top-level-function — prefer `function foo() {}` over
//! `const foo = () => {}` at module top-level.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "top-level-function",
    description: "Top-level arrow-function variables hide their name in stack traces and \
                  prevent hoisting — use a function declaration instead.",
    remediation: "Replace `const foo = (…) => { … }` at module scope with \
                  `function foo(…) { … }`. Keep arrow functions for callbacks and inline expressions.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["style"],
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
