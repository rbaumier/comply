//! html-require-img-alt
//!
//! Flags `<img>` elements without an `alt` attribute. Narrower than
//! `a11y-alt-text` (which also covers `<area>` and `<input
//! type="image">`); useful as an HTML-level, cheaper-to-reason-about
//! check.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-img-alt",
    description: "`<img>` elements must declare an `alt` attribute.",
    remediation: "Add `alt=\"<description>\"` for meaningful images or `alt=\"\"` for decorative ones.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y"],

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
