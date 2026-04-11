//! tanstack-query-array-key — query keys must be arrays.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-array-key",
    description: "TanStack Query keys must be arrays, not strings.",
    remediation: "Wrap the string in brackets: `queryKey: ['todos']`. \
                  v5 requires arrays, and hierarchical invalidation \
                  (`invalidateQueries({ queryKey: ['todos'] })`) only \
                  works on array keys.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "tanstack"],
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
