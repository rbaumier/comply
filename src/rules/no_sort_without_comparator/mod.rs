//! no-sort-without-comparator

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-sort-without-comparator",
    description: "`.sort()` without comparator sorts lexicographically.",
    remediation: "Pass an explicit comparator: `arr.sort((a, b) => a - b)` for numbers. Default `.sort()` converts to strings, so `[10, 2, 1].sort()` yields `[1, 10, 2]`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}
