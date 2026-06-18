//! ui-no-inline-bounce-easing — bounce/elastic easing feels dated; real
//! objects decelerate smoothly.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-inline-bounce-easing",
    description: "Bounce/elastic easing in inline styles — use `ease-out` or a smooth deceleration curve.",
    remediation: "Replace bounce/elastic/wobble easing with `ease-out` or \
                  `cubic-bezier(0.16, 1, 0.3, 1)` for a modern feel.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],

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
