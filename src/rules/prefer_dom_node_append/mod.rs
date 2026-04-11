//! prefer-dom-node-append

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-append",
    description: "Prefer `Node#append()` over `Node#appendChild()`.",
    remediation: "Replace `.appendChild(x)` with `.append(x)`. \
                  `.append()` accepts multiple arguments, strings, and \
                  never returns the appended node (avoiding subtle misuse).",
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
