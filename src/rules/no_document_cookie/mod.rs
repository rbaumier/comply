//! no-document-cookie

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-document-cookie",
    description: "Do not use `document.cookie` directly.",
    remediation: "Use a cookie library (e.g. `js-cookie`, `cookie`) instead of raw `document.cookie` access. Direct cookie manipulation is error-prone and hard to maintain.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
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
