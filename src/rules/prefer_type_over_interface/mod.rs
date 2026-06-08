//! prefer-type-over-interface — default to `type`, `interface` only for extends.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-type-over-interface",
    description: "Prefer `type` over `interface` unless you need extension.",
    remediation: "Replace `interface X { ... }` with `type X = { ... }`. \
                  Types support unions, intersections, mapped types, and \
                  conditional types. Keep `interface` only when you need \
                  `extends` or declaration merging.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
