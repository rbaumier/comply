//! prefer-top-level-await — flag async IIFE and `async main(); main()` patterns.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-top-level-await",
    description: "Prefer top-level await over async IIFE or async-function-then-call patterns.",
    remediation: "Use top-level `await` directly instead of wrapping in an async IIFE \
                  or defining an async function and immediately calling it. \
                  Top-level await is supported in ESM.",
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
