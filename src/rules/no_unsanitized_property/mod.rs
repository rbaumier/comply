//! no-unsanitized-property — flag unsafe assignments to `innerHTML`,
//! `outerHTML`, or `srcdoc` where the right-hand side is not a static
//! string literal. Any non-literal value is a potential XSS vector.
//!
//! Skipped in test files: XSS has no attack surface in test code (e.g. jsdom
//! SSR/hydration setup assigning `innerHTML` to simulate server-rendered HTML).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unsanitized-property",
    description: "Assigning a non-literal value to `innerHTML`, `outerHTML`, or `srcdoc` is an XSS vector.",
    remediation: "Use textContent, or sanitize HTML before assignment",
    severity: Severity::Error,
    doc_url: Some(
        "https://developer.mozilla.org/en-US/docs/Web/API/Element/innerHTML#security_considerations",
    ),
    categories: &["security"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
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
