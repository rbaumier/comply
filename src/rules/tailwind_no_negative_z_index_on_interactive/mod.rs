//! tailwind-no-negative-z-index-on-interactive — flag interactive
//! elements (`<button>`, `<a>`, `[role="button"]`) with `-z-*` classes.
//! A negative z-index sends them behind their stacking context, which
//! breaks pointer events.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-negative-z-index-on-interactive",
    description: "Negative `z-index` on interactive elements blocks pointer events.",
    remediation: "Remove the `-z-*` class, or use a stacking context wrapper.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["tailwind", "accessibility"],
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
