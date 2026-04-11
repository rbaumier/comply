//! prefer-set-has — flag array `.includes()` inside loops → use `Set#has()`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-set-has",
    description:
        "Prefer `Set#has()` over `Array#includes()` when checking for existence or non-existence.",
    remediation: "Convert the array to a `Set` and use `.has()` instead of \
                  `.includes()`. `Array#includes()` is O(n) per call; \
                  `Set#has()` is O(1). This matters when the check is inside \
                  a loop or called repeatedly.",
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
