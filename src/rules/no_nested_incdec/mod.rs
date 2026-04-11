//! no-nested-incdec

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-nested-incdec",
    description: "`++` or `--` used inside an expression, not as a standalone statement.",
    remediation: "Separate the increment/decrement from the expression. Write `i++; arr[i] = x;` instead of `arr[i++] = x;` to make the order of operations explicit.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
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
