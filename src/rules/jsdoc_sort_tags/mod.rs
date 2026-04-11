//! jsdoc-sort-tags

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-sort-tags",
    description: "JSDoc tags must follow canonical order: `@param` before `@returns` before `@throws` before `@example`.",
    remediation: "Reorder the tags: `@param` first, then `@returns`, then `@throws`, then `@example`. Consistent ordering makes JSDoc blocks scannable.",
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
