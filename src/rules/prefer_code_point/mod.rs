//! prefer-code-point

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-code-point",
    description: "Prefer `String#codePointAt()` over `String#charCodeAt()` and `String.fromCodePoint()` over `String.fromCharCode()`.",
    remediation: "Use `codePointAt()` instead of `charCodeAt()` and `String.fromCodePoint()` instead of `String.fromCharCode()`. The code-point variants handle full Unicode (including astral symbols) correctly.",
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
