//! prefer-keyboard-event-key

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-keyboard-event-key",
    description: "Prefer `KeyboardEvent#key` over `KeyboardEvent#keyCode`.",
    remediation: "Use `event.key` instead of `event.keyCode`, `event.charCode`, or `event.which`. The `.key` property returns a human-readable string and is the modern standard.",
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
