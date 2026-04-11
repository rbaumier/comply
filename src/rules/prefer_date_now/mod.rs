//! prefer-date-now — prefer `Date.now()` over `new Date().getTime()` and similar patterns.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-date-now",
    description: "Prefer `Date.now()` over `new Date().getTime()`, `+new Date()`, or `Number(new Date())`.",
    remediation: "Replace with `Date.now()`. It is clearer, avoids allocating a throwaway `Date` object, and is faster.",
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
