//! hono-secure-headers-disabled

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "hono-secure-headers-disabled",
    description: "Security header explicitly disabled in `secureHeaders()`.",
    remediation: "Don't disable security headers. Each one protects against a specific attack vector (HSTS, clickjacking, MIME sniffing, fingerprinting).",
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
