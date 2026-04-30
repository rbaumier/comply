//! no-null — flag `null` literal usage.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-null",
    description: "Use `undefined` instead of `null`.",
    remediation: "Replace `null` with `undefined`. Having two nullish values \
                  in the language is a footgun — standardize on `undefined` to \
                  reduce null-check surface area.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
