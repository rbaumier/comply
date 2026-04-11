//! prefer-response-static-json — prefer `Response.json()` over `new Response(JSON.stringify())`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-response-static-json",
    description: "Prefer `Response.json()` over `new Response(JSON.stringify())`.",
    remediation: "Replace `new Response(JSON.stringify(data), ...)` with \
                  `Response.json(data, ...)`. The static method sets the \
                  `Content-Type` header automatically and is more readable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
