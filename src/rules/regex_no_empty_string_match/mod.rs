//! regex-no-empty-string-match

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY_AND_RUST};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-string-match",
    description: "Regex that matches the empty string used in `.split()` or `.replace()`.",
    remediation: "A pattern like `*`, `?`, or `{0,}` can match zero characters, causing unexpected splits or replacements. Use `+` or `{1,}` instead, or add anchors.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY_AND_RUST
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
