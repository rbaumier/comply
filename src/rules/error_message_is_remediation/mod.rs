//! error-message-is-remediation

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "error-message-is-remediation",
    description: "Error messages should describe what went wrong and what to do about it.",
    remediation: "Replace short/noun-only error messages like `\"Invalid\"` or `\"Not found\"` with actionable messages: `\"User not found — verify the ID and retry\"`. Good errors contain a verb and guide the reader toward a fix.",
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
