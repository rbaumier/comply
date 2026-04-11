//! prefer-set-size — flag `[...mySet].length` → `mySet.size`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-set-size",
    description: "Prefer `Set#size` instead of spreading into an array and reading `.length`.",
    remediation: "Replace `[...mySet].length` or `Array.from(mySet).length` \
                  with `mySet.size`. Spreading a Set into an array just to \
                  read its length is wasteful — `Set#size` is O(1).",
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
