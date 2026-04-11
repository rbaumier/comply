//! prefer-structured-clone — prefer `structuredClone()` over `JSON.parse(JSON.stringify())`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-structured-clone",
    description:
        "Prefer `structuredClone(…)` over `JSON.parse(JSON.stringify(…))` for deep cloning.",
    remediation: "Replace `JSON.parse(JSON.stringify(x))` with `structuredClone(x)`. \
                  `structuredClone` handles circular references, typed arrays, and \
                  other values that JSON serialization silently drops or corrupts.",
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
