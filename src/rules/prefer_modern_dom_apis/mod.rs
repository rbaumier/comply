//! prefer-modern-dom-apis

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-modern-dom-apis",
    description: "Prefer `.before()` / `.replaceWith()` over `.insertBefore()` / `.replaceChild()`.",
    remediation: "Replace `parent.insertBefore(newNode, ref)` with `ref.before(newNode)` \
                  and `parent.replaceChild(newNode, old)` with `old.replaceWith(newNode)`. \
                  The modern APIs are called on the target node directly, removing the \
                  need for a parent reference.",
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
