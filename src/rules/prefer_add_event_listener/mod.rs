//! prefer-add-event-listener

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-add-event-listener",
    description: "Prefer `.addEventListener()` over `on`-event property assignment.",
    remediation: "Replace `element.onclick = handler` with `element.addEventListener('click', handler)`. `addEventListener` supports multiple listeners and provides better control via options (capture, passive, once).",
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
