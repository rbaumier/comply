//! inconsistent-function-call — SonarJS S3686.
//!
//! A function must be called consistently: either always with `new`
//! (constructor usage) or always without. Mixing both styles is almost
//! certainly a bug — either a missing `new` (the caller gets `undefined`
//! back on a constructor that sets `this.*`) or a stray `new` on a plain
//! function (which allocates an unused object).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "inconsistent-function-call",
    description: "A function must be called consistently — always with `new` or always without.",
    remediation: "Pick one style per function. If it sets `this.*`, always call with `new`; otherwise, never use `new`.",
    severity: Severity::Error,
    doc_url: Some("https://sonarsource.github.io/rspec/#/rspec/S3686"),
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
