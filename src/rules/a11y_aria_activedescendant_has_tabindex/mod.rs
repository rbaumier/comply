//! a11y-aria-activedescendant-has-tabindex

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-aria-activedescendant-has-tabindex",
    description: "Elements with `aria-activedescendant` must be tabbable.",
    remediation: "Add `tabIndex={0}` (or another non-negative value) to the element that uses `aria-activedescendant`.",
    severity: Severity::Error,
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
