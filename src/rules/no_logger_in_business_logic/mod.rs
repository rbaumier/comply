//! no-logger-in-business-logic

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-logger-in-business-logic",
    description: "Logging calls in business logic (service/domain/core/model/entity layers).",
    remediation: "Remove direct `logger.*` / `console.log` calls from business logic. Use a `withLogging()` wrapper or emit domain events instead. Logging is a cross-cutting concern — it belongs in infrastructure, not domain code.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
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
