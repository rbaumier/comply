//! hono-cookie-no-samesite

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-samesite",
    description: "Cookie without `sameSite` or with `sameSite: 'None'` — vulnerable to CSRF.",
    remediation: "Set `sameSite: 'Lax'` (default for most cases) or `sameSite: 'Strict'` for sensitive cookies.",
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
