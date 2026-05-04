//! ui-no-disabled-zoom — `<meta name="viewport">` with `user-scalable=no` or
//! `maximum-scale=1` prevents pinch-to-zoom, an accessibility violation.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-disabled-zoom",
    description: "Viewport meta disables pinch-to-zoom — accessibility violation.",
    remediation: "Remove `user-scalable=no` and `maximum-scale=1` from the viewport \
                  meta tag. Users with low vision rely on zoom.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
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
