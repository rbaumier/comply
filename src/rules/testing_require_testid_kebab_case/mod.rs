//! testing-require-testid-kebab-case — enforce kebab-case for `data-testid`
//! / `data-test` attribute values.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-require-testid-kebab-case",
    description: "data-testid / data-test values must be kebab-case for consistent, selector-safe querying.",
    remediation: "Use lowercase letters, digits, and hyphens only (e.g. 'submit-button', 'user-card-name').",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
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
