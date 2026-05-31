//! prefer-concise-arrow-body — flag `() => { return expr }` -> `() => expr`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::language::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-concise-arrow-body",
    description: "A block-bodied arrow that only returns a value can use a concise body.",
    remediation: "Replace `() => { return expr; }` with `() => expr` \
                  (wrap an object literal in parentheses: `() => ({ ... })`).",
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
