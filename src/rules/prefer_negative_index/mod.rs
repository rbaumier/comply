//! prefer-negative-index

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-negative-index",
    description: "Prefer negative index over `.length - index` for `slice`, `splice`, `at`, `with`, and related methods.",
    remediation: "Use a negative index directly (e.g. `str.slice(-3)`) instead of computing `.length - N`. Negative indices are shorter and less error-prone.",
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
