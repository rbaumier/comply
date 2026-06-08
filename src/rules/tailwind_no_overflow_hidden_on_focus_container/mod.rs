//! tailwind-no-overflow-hidden-on-focus-container — `overflow-hidden`
//! clips focus rings on children. Most a11y bugs trace back to this.

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-overflow-hidden-on-focus-container",
    description: "`overflow-hidden` clips focus rings on focusable children.",
    remediation: "Use `overflow-clip` (Tailwind 3.1+) or move clipping to a wrapper that doesn't host focusable children.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "accessibility"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
