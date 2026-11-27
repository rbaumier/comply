//! Flags `@container` plus an unnamed `@<size>:` variant on one element.
//! A container query never matches its own container.
//! Both classes must share a branch or be unconditional.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-self-container-query",
    description: "A container query variant never matches the element that declares `@container`.",
    remediation: "Move `@container` to a parent element, or name the target container.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["tailwind"],

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
