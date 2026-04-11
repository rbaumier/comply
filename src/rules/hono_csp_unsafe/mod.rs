//! hono-csp-unsafe

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "hono-csp-unsafe",
    description: "`unsafe-inline` or `unsafe-eval` in CSP defeats its purpose.",
    remediation: "Use nonces (`NONCE` from `hono/secure-headers`) instead of `unsafe-inline`. Avoid `unsafe-eval` — it enables code injection.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "hono"],
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
