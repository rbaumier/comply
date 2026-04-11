//! prefer-dom-node-remove

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-dom-node-remove",
    description: "Prefer `childNode.remove()` over `parentNode.removeChild(childNode)`.",
    remediation: "Replace `parent.removeChild(child)` with `child.remove()`. \
                  The modern `.remove()` API is simpler and doesn't require \
                  a reference to the parent node.",
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
