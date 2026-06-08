//! a11y-anchor-is-valid

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
    id: "a11y-anchor-is-valid",
    description: "Anchors must have a valid `href` — not `\"#\"`, `\"javascript:\"`, or missing.",
    remediation: "Use a real URL for `href`, or use a `<button>` if the element triggers an action.",
    severity: Severity::Error,
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
