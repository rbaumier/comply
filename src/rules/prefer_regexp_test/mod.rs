//! prefer-regexp-test — prefer `RegExp#test()` over `String#match()` in boolean contexts.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-regexp-test",
    description: "Prefer `RegExp#test()` over `String#match()` in boolean contexts.",
    remediation: "Use `/pattern/.test(str)` instead of `str.match(/pattern/)` when only a boolean result is needed. `test()` is faster because it stops at the first match.",
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
