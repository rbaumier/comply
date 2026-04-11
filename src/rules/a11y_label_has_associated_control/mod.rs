//! a11y-label-has-associated-control

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-label-has-associated-control",
    description: "`<label>` must have an associated control via `htmlFor` or by wrapping an input.",
    remediation: "Add `htmlFor=\"input-id\"` to the `<label>` or wrap an `<input>`, `<select>`, or `<textarea>` inside it.",
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
