//! prefer-dom-node-text-content

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-text-content",
    description: "Prefer `.textContent` over `.innerText`.",
    remediation: "Replace `.innerText` with `.textContent`. \
                  `.textContent` is faster (no layout reflow), works on all \
                  node types, and returns text from hidden elements too.",
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
