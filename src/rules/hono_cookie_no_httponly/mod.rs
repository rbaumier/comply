//! hono-cookie-no-httponly

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "hono-cookie-no-httponly",
    description: "Cookie set without `httpOnly` — accessible to JavaScript (XSS vector).",
    remediation: "Add `httpOnly: true` to cookie options: `setCookie(c, name, value, { httpOnly: true, secure: true, sameSite: 'Lax' })`.",
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
