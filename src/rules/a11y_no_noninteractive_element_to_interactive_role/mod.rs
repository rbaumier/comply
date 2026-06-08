//! a11y-no-noninteractive-element-to-interactive-role

mod oxc_typescript;
#[cfg(test)]
mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-noninteractive-element-to-interactive-role",
    description: "Non-interactive elements must not be assigned interactive ARIA roles.",
    remediation: "Use a native interactive element (`<button>`, `<a>`) instead of adding an interactive `role` to a `<div>`, `<span>`, etc.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],

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
            (Language::Vue, Backend::Text(Box::new(vue::Check))),
        ],
    }
}
