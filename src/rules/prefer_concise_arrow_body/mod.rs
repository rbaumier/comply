//! prefer-concise-arrow-body — collapse `() => { return expr }` into `() => expr`.

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{Language, RuleDef};

pub mod oxc_typescript;
#[cfg(test)]
mod typescript;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-concise-arrow-body",
    description: "A block-bodied arrow that only returns a value can use a concise body.",
    remediation: "Replace `() => { return expr; }` with `() => expr` \
                  (wrap an object literal in parentheses: `() => ({ ... })`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["style", "readability"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::oxc(oxc_typescript::Check)),
            (Language::Tsx, Backend::oxc(oxc_typescript::Check)),
            (Language::JavaScript, Backend::oxc(oxc_typescript::Check)),
        ],
    }
}
