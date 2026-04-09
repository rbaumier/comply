//! no-verb-in-rest-url — REST URLs are resources, not verbs.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-verb-in-rest-url",
    description: "REST URLs should identify resources, not actions.",
    remediation: "Replace verb-in-URL patterns with HTTP semantics: \
                  `POST /api/orders` to create, `GET /api/orders/:id` to \
                  read, `PATCH /api/orders/:id` to update, \
                  `DELETE /api/orders/:id` to remove.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
