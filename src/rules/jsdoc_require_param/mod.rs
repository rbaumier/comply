//! jsdoc-require-param

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-require-param",
    description: "JSDoc block must document every function parameter with `@param`.",
    remediation: "Add a `@param` tag for each undocumented parameter. Callers rely on JSDoc to understand the API without reading implementation.",
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
