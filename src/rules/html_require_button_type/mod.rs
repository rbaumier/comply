//! html-require-button-type
//!
//! Flags `<button>` elements that do not declare an explicit `type`
//! attribute. Without `type`, browsers default to `submit` inside a
//! form, causing accidental submissions.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-require-button-type",
    description: "`<button>` must have an explicit `type` attribute.",
    remediation: "Add `type=\"button\"`, `type=\"submit\"`, or `type=\"reset\"` to the `<button>` element.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["a11y"],
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
