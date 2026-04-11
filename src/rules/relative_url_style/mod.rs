//! relative-url-style

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "relative-url-style",
    description: "Remove the `./` prefix from relative URLs in `new URL()`.",
    remediation: "Remove the leading `./` from the first argument of `new URL()`: \
                  use `new URL('file.js', base)` instead of `new URL('./file.js', base)`. \
                  The `./` is redundant in URL resolution.",
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
