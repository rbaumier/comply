//! html-no-positive-tabindex
//!
//! Flags HTML `tabindex` attribute (lowercase) with a positive value.
//! Complements `a11y-tabindex-no-positive` which targets the JSX
//! `tabIndex` camelCase attribute.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-positive-tabindex",
    description: "HTML `tabindex` attribute must not be positive — it breaks natural tab order.",
    remediation: "Use `tabindex=\"0\"` (or `-1`) and rely on document order for focus sequence.",
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
