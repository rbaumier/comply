//! consistent-existence-index-check

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "consistent-existence-index-check",
    description: "Enforce `=== -1` / `!== -1` for index existence checks.",
    remediation: "Use `index === -1` to check non-existence and `index !== -1` to check existence, instead of `< 0`, `>= 0`, or `> -1`.",
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
