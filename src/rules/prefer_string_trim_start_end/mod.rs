//! prefer-string-trim-start-end

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-trim-start-end",
    description: "Prefer `String#trimStart()` / `String#trimEnd()` over the deprecated `trimLeft()` / `trimRight()`.",
    remediation: "Replace `.trimLeft()` with `.trimStart()` and `.trimRight()` with `.trimEnd()`. \
                  The `trimLeft`/`trimRight` aliases are deprecated in favor of the spec names.",
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
