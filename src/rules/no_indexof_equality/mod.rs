//! Prefer `includes()`/`startsWith()` over `indexOf()` comparisons.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-indexof-equality",
    description: "Prefer `includes()`/`startsWith()` over `indexOf()` equality checks.",
    remediation: "Use `str.includes(x)` instead of `str.indexOf(x) !== -1`, `str.startsWith(x)` instead of `str.indexOf(x) === 0`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["e18e", "modernization"],
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
