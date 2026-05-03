//! no-await-in-promise-methods — flag `await` inside `Promise.all/race/any/allSettled` arrays.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-await-in-promise-methods",
    description: "Promise in `Promise.all/race/any/allSettled()` should not be awaited.",
    remediation: "Remove the `await` keyword from array elements passed to Promise methods. \
                  Awaiting inside the array serializes the calls, defeating the purpose of \
                  `Promise.all()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
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
