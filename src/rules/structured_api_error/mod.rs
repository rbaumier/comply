//! structured-api-error

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "structured-api-error",
    description: "Bare `new Error()` in route handlers — use structured errors.",
    remediation: "Replace `new Error(\"message\")` with a structured error containing `{ type, code, status, detail }`. Bare Error messages are not machine-parseable and lack HTTP status context.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
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
