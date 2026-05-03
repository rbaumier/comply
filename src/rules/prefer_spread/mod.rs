//! prefer-spread

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-spread",
    description: "Prefer the spread operator over `Array.from()`, `Array#concat()`, and `Array#slice()`.",
    remediation: "Use `[...x]` instead of `Array.from(x)`, `[...arr, ...other]` instead of `arr.concat(other)`, and `[...arr]` instead of `arr.slice()`. The spread syntax is more idiomatic.",
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
