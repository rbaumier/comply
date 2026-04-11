//! prefer-number-properties — prefer `Number` static properties over globals.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-number-properties",
    description: "Prefer `Number.isNaN()`, `Number.parseInt()`, etc. over global equivalents.",
    remediation: "Replace global `isNaN()`, `isFinite()`, `parseInt()`, `parseFloat()`, `NaN`, \
                  and `Infinity` with their `Number.*` equivalents. The `Number` methods are \
                  stricter (no implicit coercion) and the properties are unambiguous.",
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
