//! regex-no-multiple-spaces

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY_AND_RUST};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-multiple-spaces",
    description: "Multiple consecutive spaces in regex are hard to read and count.",
    remediation: "Use a quantifier: ` {2}` or `\\s{2,}` instead of multiple spaces.",
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
