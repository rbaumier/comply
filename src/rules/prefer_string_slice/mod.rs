//! prefer-string-slice

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-slice",
    description: "Prefer `String#slice()` over `String#substr()` and `String#substring()`.",
    remediation: "Replace `.substring()` / `.substr()` with `.slice()`. \
                  `.slice()` has clearer negative-index semantics and is the modern standard.",
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
