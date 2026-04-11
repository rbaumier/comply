//! a11y-anchor-ambiguous-text

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-anchor-ambiguous-text",
    description: "Flag `<a>` elements with ambiguous text like \"click here\" or \"read more\".",
    remediation: "Use descriptive link text that indicates the purpose of the link, e.g., \"View documentation\" instead of \"click here\".",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
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
