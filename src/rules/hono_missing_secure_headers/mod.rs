//! hono-missing-secure-headers

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "hono-missing-secure-headers",
    description: "Hono app without `secureHeaders()` middleware.",
    remediation: "Add `import { secureHeaders } from 'hono/secure-headers'` and `app.use(secureHeaders())`. This sets HSTS, X-Frame-Options, and 10+ other security headers with safe defaults.",
    severity: Severity::Warning,
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
