//! jsdoc-returns-check

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-returns-check",
    description: "`@returns` on a function that never returns a value.",
    remediation: "Remove the `@returns` tag — the function is void. A stale `@returns` misleads callers into expecting a return value.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}
