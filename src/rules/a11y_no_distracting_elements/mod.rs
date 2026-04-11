//! a11y-no-distracting-elements

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-distracting-elements",
    description: "Flag `<marquee>` and `<blink>` elements which are distracting and deprecated.",
    remediation: "Remove `<marquee>` and `<blink>` elements. Use CSS animations if motion is needed, with `prefers-reduced-motion` support.",
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
