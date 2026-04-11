//! no-try-statements

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-try-statements",
    description: "`try` blocks obscure error flow — prefer Result types or explicit error handling.",
    remediation: "Use a Result/Either type, or a wrapper function that returns `{ data, error }` tuples instead of try/catch.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
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
