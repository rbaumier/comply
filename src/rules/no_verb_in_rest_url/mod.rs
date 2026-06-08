//! no-verb-in-rest-url — REST URLs are resources, not verbs.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;
mod verb_url_match;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-verb-in-rest-url",
    description: "REST URLs should identify resources, not actions.",
    remediation: "Replace verb-in-URL patterns with HTTP semantics: \
                  `POST /api/orders` to create, `GET /api/orders/:id` to \
                  read, `PATCH /api/orders/:id` to update, \
                  `DELETE /api/orders/:id` to remove.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
