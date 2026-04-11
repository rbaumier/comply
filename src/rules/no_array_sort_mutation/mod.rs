//! no-array-sort-mutation

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-array-sort-mutation",
    description: "Prefer `Array#toSorted()` over `Array#sort()` (mutates in place).",
    remediation: "Replace `.sort()` with `.toSorted()`. `Array#sort()` mutates the \
                  array in place which can cause subtle bugs. `Array#toSorted()` \
                  returns a new sorted array, leaving the original unchanged.",
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
