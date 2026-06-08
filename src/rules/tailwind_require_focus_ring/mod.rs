//! tailwind-require-focus-ring — keyboard users need a visible focus
//! indicator on every interactive element. Require a `focus:ring-*` /
//! `focus-visible:ring-*` class on buttons, anchors, form controls, and
//! `role="button"` elements.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-require-focus-ring",
    description: "Interactive elements must carry a `focus:ring-*` class for keyboard a11y.",
    remediation: "Add `focus:ring-2` (and ideally `focus:ring-offset-2`, `focus:outline-none`) to buttons, anchors, inputs, selects, textareas, and role=button elements.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind", "a11y"],

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
