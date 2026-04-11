//! hono-cors-permissive

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "hono-cors-permissive",
    description: "Permissive CORS allows any origin to access the API.",
    remediation: "Restrict `cors({ origin: 'https://your-domain.com' })`. Default `cors()` sets `origin: '*'`. With `credentials: true`, the origin must be explicit.",
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
