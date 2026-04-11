//! prefer-at

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-at",
    description: "Prefer `.at()` method for index access and `String#charAt()`.",
    remediation: "Use `.at(-1)` instead of `[arr.length - 1]` for last-element access, and `str.at(0)` instead of `str.charAt(0)`. The `.at()` method handles negative indices natively.",
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
