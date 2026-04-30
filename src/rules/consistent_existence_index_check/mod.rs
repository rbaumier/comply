//! consistent-existence-index-check

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "consistent-existence-index-check",
    description: "Enforce `=== -1` / `!== -1` for index existence checks.",
    remediation: "Use `index === -1` to check non-existence and `index !== -1` to check existence, instead of `< 0`, `>= 0`, or `> -1`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
