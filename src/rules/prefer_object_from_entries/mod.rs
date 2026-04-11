//! prefer-object-from-entries

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-object-from-entries",
    description: "Prefer `Object.fromEntries()` over building objects from key-value pairs via `reduce`.",
    remediation: "Use `Object.fromEntries(arr.map(…))` instead of `arr.reduce((acc, …) => ({ ...acc, … }), {})`. It is more readable and avoids quadratic spread copies.",
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
