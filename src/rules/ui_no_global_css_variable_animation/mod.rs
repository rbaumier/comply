//! ui-no-global-css-variable-animation — `document.documentElement.style.setProperty`
//! inside `requestAnimationFrame` triggers full-page style recalc every frame.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-global-css-variable-animation",
    description: "Global CSS variable change inside `requestAnimationFrame` triggers full-page recalc.",
    remediation: "Scope the CSS variable to the animated element: \
                  `element.style.setProperty('--x', value)` instead of \
                  `document.documentElement.style.setProperty('--x', value)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["performance"],

    skip_in_test_dir: false,
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
