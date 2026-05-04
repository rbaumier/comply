//! Prefer `URL.canParse()` over try-catch `new URL()` (ES2024).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-url-canparse",
    description: "Prefer `URL.canParse(url)` over try-catch with `new URL()`.",
    remediation: "Replace try-catch URL validation with `URL.canParse(url)` (available in modern runtimes).",
    severity: Severity::Warning,
    doc_url: Some("https://developer.mozilla.org/en-US/docs/Web/API/URL/canParse_static"),
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
