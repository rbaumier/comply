//! hono-csrf-missing

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "hono-csrf-missing",
    description: "Mutation routes without CSRF protection.",
    remediation: "Add `import { csrf } from 'hono/csrf'` and `app.use(csrf())` to protect mutation endpoints against cross-site request forgery.",
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
