//! a11y-no-aria-hidden-on-focusable

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-aria-hidden-on-focusable",
    description: "Flag `aria-hidden=\"true\"` on focusable elements.",
    remediation: "Remove `aria-hidden` from focusable elements or remove the focusable behavior. Elements hidden from assistive technology should not be focusable.",
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
