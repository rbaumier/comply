//! prefer-single-call — combine consecutive `.push()` / `.classList.add()` / `.classList.remove()` calls.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-single-call",
    description: "Combine multiple consecutive `.push()`, `.classList.add()`, or `.classList.remove()` into one call.",
    remediation: "Merge consecutive calls to the same method on the same receiver \
                  into a single call with multiple arguments. For example, \
                  `arr.push(a); arr.push(b);` becomes `arr.push(a, b);`.",
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
