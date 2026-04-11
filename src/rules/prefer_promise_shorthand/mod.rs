//! prefer-promise-shorthand

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-promise-shorthand",
    description: "`new Promise` wrapping a single `resolve`/`reject` call — use `Promise.resolve`/`Promise.reject` instead.",
    remediation: "Replace `new Promise((resolve) => resolve(x))` with `Promise.resolve(x)` and `new Promise((_, reject) => reject(x))` with `Promise.reject(x)`.",
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
