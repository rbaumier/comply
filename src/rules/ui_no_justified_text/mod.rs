//! ui-no-justified-text — `textAlign: 'justify'` without `hyphens: 'auto'`
//! produces rivers of whitespace and harms readability.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-justified-text",
    description: "`textAlign: 'justify'` without `hyphens: 'auto'` — produces rivers of whitespace.",
    remediation: "Either drop `textAlign: 'justify'` or pair it with `hyphens: 'auto'` so the \
                  browser can break long words and avoid awkward spacing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
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
