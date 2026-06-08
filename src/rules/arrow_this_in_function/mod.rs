//! arrow-this-in-function — flag `this` inside an arrow function that has
//! no enclosing regular function/method to bind `this`.
//!
//! Arrow functions don't create their own `this`; they inherit it from the
//! surrounding lexical scope. When an arrow function sits at module scope
//! (or only inside other arrows), its `this` is effectively `undefined`
//! (strict) or the global object — almost always a bug.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "arrow-this-in-function",
    description: "`this` inside an arrow function with no enclosing \
                  regular function or method binds to the outer scope \
                  and is likely a bug.",
    remediation: "Arrow functions don't bind their own `this`, use regular \
                  function or ensure nested in function context",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],

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
