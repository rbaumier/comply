//! prefer-string-slice

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-string-slice",
    description: "Prefer `String#slice()` over `String#substr()` and `String#substring()`.",
    remediation: "Replace `.substring()` / `.substr()` with `.slice()`. \
                  `.slice()` has clearer negative-index semantics and is the modern standard.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
