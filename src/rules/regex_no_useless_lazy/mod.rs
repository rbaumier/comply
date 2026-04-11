//! regex-no-useless-lazy

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-useless-lazy",
    description: "Lazy quantifier has no effect when the quantified token can only match a single length.",
    remediation: "Remove the `?` after the quantifier — it has no effect here.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
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
