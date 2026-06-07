//! react-form-requires-novalidate — native `<form>` without `noValidate`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-form-requires-novalidate",
    description: "A native `<form>` without `noValidate` lets the browser run its own HTML \
                  validation in parallel with the app's validation layer, producing two \
                  competing error UXs.",
    remediation: "Add `noValidate` to the `<form>` so the app's client-side validation \
                  owns the error experience end to end.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
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
