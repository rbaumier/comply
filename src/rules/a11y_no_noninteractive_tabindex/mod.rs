//! a11y-no-noninteractive-tabindex

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-no-noninteractive-tabindex",
    description: "Flag non-interactive elements with `tabIndex` (other than -1).",
    remediation: "Remove `tabIndex` from non-interactive elements or use a native interactive element. `tabIndex={-1}` is acceptable for programmatic focus.",
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
