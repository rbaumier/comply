//! a11y-control-has-associated-label

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "a11y-control-has-associated-label",
    description: "Interactive elements must have an accessible label.",
    remediation: "Add text content, `aria-label`, or `aria-labelledby` to `<button>`, `<input>`, `<select>`, and `<textarea>` elements. `<input type=\"hidden\">` is exempt.",
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
