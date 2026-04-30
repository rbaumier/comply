//! Prefer passing timer arguments directly instead of wrapping in arrow function.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-timer-args",
    description: "Prefer `setTimeout(fn, delay, arg)` over `setTimeout(() => fn(arg), delay)`.",
    remediation: "Pass arguments directly to setTimeout/setInterval: `setTimeout(fn, delay, arg1, arg2)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["e18e", "modernization"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
