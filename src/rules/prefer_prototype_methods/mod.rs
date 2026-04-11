//! prefer-prototype-methods — borrow methods from prototypes, not literal instances.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-prototype-methods",
    description: "Prefer borrowing methods from the prototype instead of a literal instance.",
    remediation:
        "Replace `{}.hasOwnProperty.call(…)` with `Object.prototype.hasOwnProperty.call(…)`, \
                  `[].slice.call(…)` with `Array.prototype.slice.call(…)`, etc.",
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
