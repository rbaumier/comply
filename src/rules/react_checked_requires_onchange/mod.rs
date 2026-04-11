//! react-checked-requires-onchange — checked without onChange or readOnly.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "react-checked-requires-onchange",
    description: "`checked` prop without `onChange` or `readOnly` makes the input uncontrollable.",
    remediation: "Add an `onChange` handler or `readOnly` prop. Without either, \
                  React renders a frozen checkbox/radio that the user cannot \
                  interact with, and emits a console warning.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
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
