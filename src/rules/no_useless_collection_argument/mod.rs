//! no-useless-collection-argument — flag useless arguments to Set/Map constructors.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-collection-argument",
    description: "Disallow useless values in `Set`, `Map`, `WeakSet`, or `WeakMap` constructors.",
    remediation: "Remove the empty/null/undefined argument from the collection \
                  constructor. `new Set([])` and `new Map(undefined)` are \
                  equivalent to `new Set()` and `new Map()`.",
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
