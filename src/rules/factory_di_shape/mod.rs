//! factory-di-shape

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "factory-di-shape",
    description: "`create*` factory functions should take a single deps object, not individual params.",
    remediation: "Replace individual dependency parameters with a single object: `createService({ db, cache, logger })`. A deps object makes the dependency list extensible without breaking callers and reads as named arguments.",
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
