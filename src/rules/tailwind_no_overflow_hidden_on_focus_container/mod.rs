//! tailwind-no-overflow-hidden-on-focus-container — `overflow-hidden`
//! clips focus rings on children. Most a11y bugs trace back to this.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-overflow-hidden-on-focus-container",
    description: "`overflow-hidden` clips focus rings on focusable children.",
    remediation: "Use `overflow-clip` (Tailwind 3.1+) or move clipping to a wrapper that doesn't host focusable children.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "accessibility"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::Text(Box::new(typescript::Check))),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
