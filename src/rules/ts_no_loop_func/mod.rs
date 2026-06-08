//! ts-no-loop-func — disallow function declarations/expressions inside loops.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-loop-func",
    description: "Functions declared inside loops often cause bugs due to closures capturing the loop variable by reference.",
    remediation: "Move the function outside the loop, or use `let`/`const` in a `for` loop to create a new binding per iteration.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-loop-func"),
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
