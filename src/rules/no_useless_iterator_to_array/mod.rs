//! no-useless-iterator-to-array — flag unnecessary `.toArray()` on iterators.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-iterator-to-array",
    description: "Disallow unnecessary `.toArray()` on iterators.",
    remediation: "Remove `.toArray()` — the consuming context already accepts \
                  iterables. `for…of`, spread, `yield*`, `new Set(…)`, \
                  `Array.from(…)`, and `Object.fromEntries(…)` all work \
                  directly on iterators.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
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
