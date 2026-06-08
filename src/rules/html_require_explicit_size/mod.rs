//! html-require-explicit-size
//!
//! Flags `<img>` and `<video>` elements lacking explicit `width` and
//! `height`. Reserving space reduces Cumulative Layout Shift (CLS).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-explicit-size",
    description: "`<img>` and `<video>` must declare `width` and `height` to avoid layout shift.",
    remediation: "Add explicit `width` and `height` attributes (or CSS `aspect-ratio`) to reserve space.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y", "performance"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
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
