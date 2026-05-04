//! ui-no-wide-letter-spacing — inline `letterSpacing` above 0.05em hurts
//! readability for body copy and small UI text.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-wide-letter-spacing",
    description: "Inline `letterSpacing` above 0.05em — hurts readability.",
    remediation: "Keep `letterSpacing` at or below 0.05em for body text. Reserve wider tracking \
                  for short uppercase headings only.",
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
