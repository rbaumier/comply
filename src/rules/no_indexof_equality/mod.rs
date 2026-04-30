//! Prefer `includes()`/`startsWith()` over `indexOf()` comparisons.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-indexof-equality",
    description: "Prefer `includes()`/`startsWith()` over `indexOf()` equality checks.",
    remediation: "Use `str.includes(x)` instead of `str.indexOf(x) !== -1`, `str.startsWith(x)` instead of `str.indexOf(x) === 0`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["e18e", "modernization"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
