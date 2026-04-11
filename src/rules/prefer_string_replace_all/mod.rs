//! prefer-string-replace-all

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-replace-all",
    description: "Prefer `String#replaceAll()` over `String#replace()` with a global regex.",
    remediation: "Replace `.replace(/pattern/g, replacement)` with `.replaceAll('pattern', replacement)`. \
                  `replaceAll()` is clearer in intent and avoids regex escaping pitfalls.",
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
