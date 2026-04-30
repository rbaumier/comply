//! hono-no-get-with-body — GET / HEAD handlers must not consume the request body.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "hono-no-get-with-body",
    description: "GET and HEAD requests do not have a body — calling `c.req.json()` / `text()` / `parseBody()` / `formData()` is a bug.",
    remediation: "Use query parameters or path parameters for GET/HEAD routes. Move body-consuming logic to POST/PUT/PATCH.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["hono", "correctness"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
