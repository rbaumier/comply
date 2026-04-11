//! a11y-mouse-events-have-key-events

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-mouse-events-have-key-events",
    description: "Flag `onMouseOver` without `onFocus` and `onMouseOut` without `onBlur`.",
    remediation: "Add `onFocus` alongside `onMouseOver` and `onBlur` alongside `onMouseOut` to ensure keyboard accessibility.",
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
